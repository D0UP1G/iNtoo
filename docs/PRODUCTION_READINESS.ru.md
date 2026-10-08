# Проверка готовности iNtoo к production

Дата: 2026-10-07. Объект: рабочее дерево `/home/dpg/Documents/iNtoo`.

**Решение: NEEDS CHANGES.** Найденные дефекты исправлены и закрыты регрессиями,
но выпуск пока нельзя принимать: настоящие entrypoint Quickshell в этой среде
не загружаются, Gentoo Portage сборка не была выполнена, а исходники не закреплены
в Git revision.

## Исправления

- Парсер Niri теперь исключает отключённые slash-dash блоки и startup-команды.
  Обход include-графа различает логические symlink-пути и физические файлы,
  поэтому вложенные относительные include не теряются.
- Snapshot restore и startup multi-file публикация сохраняют backup при
  неудачном откате и сообщают путь для восстановления. Snapshot учитывает только
  активные include.
- Миграция внешнего каталога конфигурации сохраняет старый путь, если копирование
  не удалось; resolver продолжает находить исходный пользовательский config.
- Staged Niri validation больше не следует абсолютной symlink из live-конфига.
- Gentoo устанавливает `uv`, нужные portal backend и tessdata; Doctor знает
  Gentoo repair atoms, настраивает OCR USE-флаги с rebuild и запускает оболочку
  через сервис/session manager. Python requirements теперь обязательны: ошибки
  создания venv или установки пакетов передаются установщику.
- Release cleanliness check включает untracked файлы.

Поведенческие тесты покрывают отключённые аргументы, два include alias с разными
relative-контекстами, сохранение backup при двойном отказе, Gentoo OCR repairs,
миграционные ошибки и lifecycle manager.

## Остаточные блокеры

`python3 scripts/test-shell-startup.py` запускает настоящий Quickshell 0.3.1 с
offscreen platform. Результат: 7 методов, 3 проходят, 4 не проходят; отсутствуют
10 QML-модулей (`Quickshell.Hyprland`, Bluetooth, Networking, Mpris,
Notifications, Pam, Pipewire, SystemTray, UPower и QtMultimedia). Все три семейства
entrypoint поэтому не загружаются. Это подтверждает блокер установленной среды,
но не проверяет успешность изменённого USE-профиля: Portage rebuild в этом аудите
не выполнялся. После сборки необходимо повторить нативный запуск.

Текущий HEAD содержит только LICENSE; остальное дерево находится в untracked
файлах. Релизная проверка теперь отклоняет такие файлы, как и должна. Для
воспроизводимого релиза всё дерево необходимо добавить в Git и проверять
собранный артефакт из чистого revision.

OpenRC и systemd тестировались контрактными проверками с заглушками, не настоящим
logout/reboot. Нужна приёмка реальных пользовательских сессий: панели, PAM,
clipboard, lock, suspend/resume, audio, tray и screencast. ShellCheck отсутствует.

## Проверки

| Проверка | Результат |
|---|---|
| `python3 scripts/test-review-regressions.py` | 36 тестов, PASS |
| `python3 scripts/test-audit-regressions.py` | 21 тест, PASS; Gentoo Portage заглушён |
| `bash scripts/test-local-distribution.sh` | PASS; 10 payload и 34 OpenRC теста |
| `python3 scripts/test-shell-startup.py` | FAIL: 4 из 7 методов из-за отсутствующих runtime modules |
| QML Qt compatibility / pitfalls | 1404 / 1354 файлов, PASS |
| Проверки idle, brightness, lock-wake | PASS |

Успешные локальные регрессии подтверждают только проверенные сценарии. До
успешной нативной сборки и запуска Gentoo, а также закрепления изменений в Git,
production-релиз не готов.
