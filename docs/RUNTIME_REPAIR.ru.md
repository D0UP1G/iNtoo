# Исправление Gentoo runtime

Дата диагностики: 2026-10-07.

## Что мешало запуску

На проверяемой машине установлен `gui-apps/quickshell-0.3.1::guru`, но его
записанные Portage USE-флаги содержали только `layer-shell sockets wayland`
(кроме системных флагов архитектуры). Проверка `qs --version` проходила,
хотя QML-модули, которые iNtoo импортирует непосредственно, отсутствовали.

| Отсутствующий импорт | Требуемый USE-флаг Quickshell | Затронутая функция |
|---|---|---|
| `Quickshell.Hyprland` | `hyprland` | Загрузка общих компонентов, включая SettingsOverlay; импорт пока обязателен и на Niri |
| `Quickshell.Bluetooth` | `bluetooth` | Управление Bluetooth |
| `Quickshell.Networking` | `networkmanager` | Сетевые подключения |
| `Quickshell.Services.Mpris` | `mpris` | Управление медиаплеерами |
| `Quickshell.Services.Notifications` | `notifications` | Уведомления |
| `Quickshell.Services.Pam` | `pam` | PAM-аутентификация |
| `Quickshell.Services.Pipewire` | `pipewire` | Громкость и аудиоустройства |
| `Quickshell.Services.SystemTray` | `tray` | Системный трей |
| `Quickshell.Services.UPower` | `upower` | Состояние питания и батареи |
| `QtMultimedia` | Отдельный пакет `dev-qt/qtmultimedia[qml]` | QML-компоненты мультимедиа |

Наличие базового импорта `Quickshell.Wayland` также не подтверждает наличие
`WlSessionLock`, `WlSessionLockSurface` и `ScreencopyView`: эти типы зависят от
`session-lock` и `screencopy`. Последний флаг оставался выключенным даже в
плане пересборки до исправления конфигурации.

В `/etc/portage/package.use/intoo` осталась старая строка Quickshell с
отключёнными функциями. Более новый `/etc/portage/package.use/intoo-session`
включал большинство функций, но не `screencopy`; установленный пакет ещё не
был пересобран с этим профилем. Поэтому прежний startup suite получил
10 ошибок импорта и 3 ошибки запуска семейств ii, Iris и Waffle.

## Подготовленное исправление

Обе строки Quickshell согласованы с обязательным набором:

```text
gui-apps/quickshell sockets wayland layer-shell session-lock toplevel-management hyprland tray pipewire mpris pam upower notifications bluetooth networkmanager screencopy
```

Прежние значения сохранены в `/var/backups/intoo-runtime-xv0hsjg6`.
Опциональные отключённые функции `X`, `i3`, `jemalloc`, `policykit`, `greetd`
и `crash-handler` сохранены в основной строке. Проверенный план Portage
содержит только два пакета: пересборку Quickshell 0.3.1 и установку
QtMultimedia 6.11.2 с `qml`, совпадающей с установленным Qt 6.11.2.

Команда сборки на этой машине:

```bash
sudo env MAKEOPTS=-j1 PORTAGE_TMPDIR=/var/tmp/intoo-runtime-build \
  emerge --ask=n --verbose --jobs=1 --load-average=2 --update --changed-use \
  gui-apps/quickshell::guru dev-qt/qtmultimedia
```

Одно задание и сборочная директория на диске выбраны из-за малого объёма
RAM без swap. После освобождения памяти компиляция возобновлена с двумя
заданиями и сохранёнными объектными файлами:

```bash
sudo env MAKEOPTS=-j2 FEATURES=keepwork \
  PORTAGE_TMPDIR=/var/tmp/intoo-runtime-build \
  emerge --resume --ask=n --verbose --jobs=1 --load-average=2
```

Логи: `/tmp/intoo-runtime-emerge.log` и
`/tmp/intoo-runtime-emerge-resume.log`.

В `scripts/test-shell-startup.py` добавлена отдельная проверка компиляции
Wayland-типов: успешный импорт модуля не должен скрывать неполную сборку.
Проверка не создаёт Wayland-поверхности и выполняется с Qt offscreen.
Doctor теперь также компилирует типы блокировки в `Component`, не создавая
поверхности и не блокируя сеанс. Раньше его probe проверял только
`ScreencopyView` и мог пропустить отключённый `session-lock`. Отдельный native
тест загружает непосредственно QML probe из Doctor в изолированной среде.

При разборе установки обнаружен дополнительный дефект: зависимости
`qt5compat` и `qtmultimedia` устанавливались без обязательного включения
`qml`. На этой машине флаг уже задан отдельным пользовательским файлом
`inir-qt`, однако на чистой системе установка могла оставить импорты
неработоспособными. В общий профиль установки и Doctor добавлены
`dev-qt/qt5compat gui qml` и `dev-qt/qtmultimedia qml`; обновлена ручная
инструкция. Регрессия проверяет установку и Doctor на OpenRC/systemd с
файлом и директорией `package.use`.

## Проверка результата

| Проверка | Результат |
|---|---|
| Регрессия Qt USE-флагов до исправления | FAIL, 8 комбинаций установки/Doctor, init и формата Portage |
| `python3 scripts/test-audit-regressions.py` после исправления | PASS, 22 tests |
| Проверка соответствия ручного и автоматического Quickshell USE | PASS |
| `make test-local` | PASS, включая 34 OpenRC tests |
| Новый native probe Wayland-типов до пересборки | FAIL: `WlSessionLockSurface is not a type` |
| `bash -n sdata/dist-gentoo/install-deps.sh` | PASS |
| ShellCheck 0.11.0 с `.shellcheckrc`, `--extended-analysis=false` | Осталось существующее SC2181; новых diagnostics нет |

Сборка выполняется. Итог будет записан после установки и повторного native
startup suite. Проверка offscreen не подтверждает работу реальной
Wayland-сессии, устройств, аудио и PAM.
