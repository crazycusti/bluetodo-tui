# BlueTodo TUI

Ein kleiner Terminal-Client für das BlueTodo-TCP-Protokoll.

## Start

- BlueTodo-Server starten und das TCP-Protokoll in `/config` aktivieren
- dann:
  - `cargo run -- --host 127.0.0.1 --port 5877 --token <optional>`

## Tasten

- Aktive Todos: `n` neu, `x` archivieren, `v` Archivansicht, `Enter` öffnen, `r` neu laden, `q` beenden
- Archiv: `u` wiederherstellen, `v` aktive Liste, `Enter` öffnen, `r` neu laden, `q` beenden
- Tasks: `a` neue Task, `t` umschalten, `b` zurück, `r` neu laden, `q` beenden

## Lizenz

MIT, siehe `LICENSE`.
