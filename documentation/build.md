# Build e execução

## Dependências

- Python 3;
- Rust **nightly** (kernel) e **stable** (ferramentas de host e componentes),
  com `rustup target add x86_64-unknown-none`;
- `xorriso`, `dosfstools` (`mkfs.vfat`) e `mtools` (`mmd`, `mcopy`) para gerar ISO;
- submódulos inicializados (`git submodule update --init --recursive`);
- QEMU x86_64 e OVMF para testar o boot UEFI;
- `qemu-img`, `sgdisk`, `mkfs.vfat` e `mtools` para criar a imagem de disco GPT.

Os scripts explicam e encerram com código de erro quando uma dependência
estiver ausente.

## Estrutura do build

O build é dividido por etapa em módulos Python em `build/`, orquestrados pelo
`build.py` da raiz (que substitui os antigos Makefiles). Cada módulo também
roda isolado:

| Módulo | Responsabilidade | Comando |
| --- | --- | --- |
| `build/common.py` | variáveis, ferramentas e artefatos compartilhados | (importado) |
| `build/userland.py` | apps, utilitários Rust/C e libc do ImagineOS | `python3 build.py userland` |
| `build/components.py` | componentes dos submódulos (shell, Liminal, desktop) | `python3 build.py components` |
| `build/ramfs.py` | árvore `ramfs/` montada em `.build/ramfs` | `python3 build.py ramfs` |
| `build/kernel.py` | kernel `dreamcore` e `dzImage` (via submódulo DNU) | `python3 build.py kernel` |
| `build/iso.py` | ISO UEFI (via `imagine-tools/isobuilder`) | `python3 build.py iso` |
| `build/run.py` | execução no QEMU/OVMF | `python3 build.py run` |

O pipeline completo é:

```sh
python3 build.py iso
```

que executa, em ordem: userland → componentes → ramfs → dzImage (kernel +
initramfs) → extração do `kernel.elf` → ISO.

## Componentes (submódulos)

Cada componente é um submódulo na raiz com um programa compilado para o alvo
bare-metal `x86_64-unknown-none`; o ELF é empacotado em **DEF** pelo
`dnu-pack` e instalado no ramfs:

| Submódulo | Binário instalado |
| --- | --- |
| `imagine-shell` | `/bin/shell` |
| `imagine-video-server` | `/bin/liminal-server` |
| `imagine-compositor` | `/bin/liminal-client` |
| `imagine-desktop` | `/bin/ede-panel` |
| `imagine-bootloader` | `BOOTX64.EFI` + `limine.conf` na ISO/ESP |
| `imagine-tools` | `isobuilder` (ferramenta de host que monta a ISO) |

## Compilar a ISO

```sh
python3 build.py iso
```

A ISO UEFI será gerada em `.build/distro/dreamcore-AAAA-MM-DD-HH-MM-astrid.iso`.
`python3 build.py kernel` publica o contêiner como `.build/dzImage`. O dzImage
é o formato de empacotamento do DNU: contém o ELF do kernel, o initramfs e a
tabela de membros. Na ISO, o Limine carrega `/boot/kernel.elf` (o ELF extraído
do dzImage por `dfs-image --dump-kernel`) e recebe `/boot/dzImage` como módulo;
o kernel lê a tabela de membros, mapeia o initramfs e carrega `/sbin/init` como
PID 1.

Os programas de `imagine-tools/programs/` e `imagine-tools/utilities/` são incluídos sem
extensão: `/sbin/init`, `/sbin/getty`, `/bin/shell`, a stack Liminal
(`/bin/liminal-server`, `/bin/liminal-client`) e comandos em `/bin`. Todos
eles são empacotados no formato DEF (magic `DNUF`) por `dnu-pack`; o kernel
não carrega ELF de userspace. A fonte PSF
8x16 `dnu/src/assets/zap-vga16.psf` é incorporada ao kernel como
fallback para uma fonte inválida ou ausente no RAMFS. Ela também é copiada
para `system/fonts/zap-vga16.psf` dentro do initramfs, de modo que o instalador
inclui a fonte na raiz DFS persistente.

Apps C em `ramfs/home/<nome>.c` implementam `main`; a libc do ImagineOS
(`imagine-common/libc/`) os compila como ELF estático, que
`dnu-pack` converte para o contêiner DEF antes de entrar no initramfs. A
interface inicial inclui `read`, `write`, `_exit`, `printf`, `puts`, entrada e
saída de caracteres, operações básicas de `<string.h>` e `<stdlib.h>`, e as
extensões Dreamcore para ler/gravar arquivos inteiros. O formatador cobre
`%s`, `%c`, inteiros decimais/hexadecimais e `%%`; o allocator é um bump arena
de 64 KiB por processo e `free` ainda não recupera memória. I/O de arquivo está
limitado a 4096 bytes por chamada; a área mutável total do ramfs é 1 MiB.

Este ainda não é um port de runtime de um compilador C nem da musl. O ImagineOS
precisa de mais syscalls POSIX, descritores de arquivo, memória dinâmica e
startup antes de executar esses projetos upstream dentro do ramfs.

## Executar com QEMU

```sh
python3 build.py run
```

O firmware OVMF é procurado em `/usr/share/OVMF`, `/usr/share/ovmf` e na raiz
do repositório. A imagem atual não inclui boot BIOS. O ESP contém `dzImage` e
`kernel.elf`; os programas de userspace existem somente dentro do initramfs.

## Drivers

Os drivers são módulos nativos do kernel DNU (`dnu/src/`): `ata` e `block`
(armazenamento), `keyboard` e `mouse` (entrada PS/2), `acpi`, `time` e
`framebuffer`. O init publica os dispositivos no VFS; os daemons de drivers
herdados do Redox foram removidos. Detalhes em
[documentation/drivers.md](documentation/drivers.md).

## Criar e iniciar uma imagem de disco GPT

O instalador de userspace (`distroinstall`, disponível no shell da ISO) grava
uma tabela GPT, uma ESP FAT16 com Limine, `dzImage` e a raiz DFS, em uma imagem
de disco alvo anexada ao QEMU. O alvo não altera discos físicos.
