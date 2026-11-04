# ImagineOS Common Software Collection

Este diretório contém código compartilhado entre os componentes do ImagineOS que **não** pertencem ao kernel DNU (`dnu/`) nem ao userspace principal (`imagine-base/`).

## Estrutura de pastas

| Pasta | Conteúdo |
|-------|----------|
| `console/` | Shell de linha de comando do ImagineOS (intérprete POSIX, builtins, histórico, edição de linha). |
| `systemfetch/` | Utilitário `systemfetch` — exibe informações do sistema (uptime, RAM, resolução, versão da ABI). |
| `utilities/` | Utilitários de sistema (cat, ls, mkdir, ps, rm, shutdown, etc.). |
| `init/init.rs` | **PID 1 (Session Manager)** — inicializa o sistema, lança o servidor de janelas (`ws_server`) e o shell, e supervisiona o ciclo de vida dos processos. |
| `gui/main.rs` | **Servidor de janelas/compositor em userspace** — executa em Ring 3. Gerencia o framebuffer, janelas, entrada (teclado/mouse), GDI simplificado e renderização ordenada por Z. |
| `gui/gdi.rs` | GDI simplificado (desenho direto no framebuffer: retângulos, texto, cursor). |
| `gui/wm.rs` | Window Manager (lista de janelas, z-order, foco, renderização ordenada). |
| `explorer/main.rs` | **Shell gráfico** — cria a taskbar, área de trabalho, menu Iniciar e gerencia janelas abertas. Roda em Ring 3 usando as APIs de userspace do ImagineOS. |

## Filosofia

O ImagineOS mantém a interface gráfica **fora do kernel** (veja `AGENTS.md`, seção 4.5). Toda a pilha gráfica roda em userspace usando as syscalls de display expostas pelo kernel DNU (`SYS_FB_INFO`, `SYS_FB_MAP`, `SYS_CONSOLE_RELEASE`, `SYS_SURFACE_*`, etc.).
