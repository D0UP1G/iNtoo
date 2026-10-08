# Senior review iNtoo

Дата: 2026-10-07. Проверено текущее рабочее дерево `/home/dpg/Documents/iNtoo`.

**Решение: NEEDS CHANGES. Production-приёмка отклонена.** Найдены 9 замечаний:
4 уровня P1 и 5 уровня P2. Три P1 связаны с сохранностью пользовательских данных.
Прежние исправления не закрывают эти дополнительные сценарии.

Проверены Gentoo installation/repair, systemd/OpenRC integration,
update/uninstall, migrations, backups, восстановление пользовательских
модификаций, пользовательские package actions и release gate. Проведены
отдельные read-only ревью lifecycle и runtime, диагностический проход и
независимое повторение ключевых воспроизведений основным агентом.
Рабочий процесс использует skill it-team; назначенная им модель Tech Lead
`gpt-6.1` недоступна в хосте, итоговая оценка выполнена основным агентом.
Это обзор рассматриваемых путей, а не гарантия отсутствия других дефектов.

## Подтверждённые замечания

### SR01 · P1 · Uninstall удаляет данные после отказа backup

Места: [uninstall.sh:466](../sdata/lib/uninstall.sh#L466),
[uninstall.sh:474](../sdata/lib/uninstall.sh#L474),
[caller:1073](../sdata/lib/uninstall.sh#L1073).

`uninstall_create_backup` игнорирует ошибки mkdir/cp и завершается успешным
echo. `run_uninstall` не проверяет результат backup перед удалением config и
пользовательского state. При ENOSPC, ошибке чтения или отказе записи операция
может уничтожить настройки без сохранённой копии.

Временная fixture исполняет настоящий `run_uninstall`: cp заменён функцией,
возвращающей 28; сервисы, UI и обработка shared resources заменены заглушками.
Результат: exit 0, config удалён, сохранённых config нет, вывод содержит
`Uninstall Complete`. Реальное переполнение диска не проверялось.

Исправление: любая ошибка обязательного backup должна прекращать uninstall;
успешный backup публиковать только после полного сохранения всех данных.

### SR02 · P1 · Миграция выполняется без обязательного backup

Места: [migrations.sh:140](../sdata/lib/migrations.sh#L140),
[migrations.sh:143](../sdata/lib/migrations.sh#L143),
[caller:350](../sdata/lib/migrations.sh#L350).

`create_backup` продолжает работу после ошибки cp и возвращает успешный echo.
В `apply_migration` статус дополнительно скрывается через `local backup_dir=$(...)`.
Миграция изменяет target; после её ошибки попытка rollback читает отсутствующую
копию и также не проверяется.

Fault injection: cp возвращает 28, migration записывает partial JSON и
возвращает ошибку. Итог: exit 1, исходный config утрачен, partial JSON остался,
backup отсутствует. Ошибочный exit не обеспечивает сохранность данных.

Исправление: отдельно проверять создание backup до запуска migration;
проверять rollback и сохранять recovery artifacts при его отказе.

### SR03 · P1 · Ошибка декодирования обнуляет shell profile

Места: [uninstall.sh:371](../sdata/lib/uninstall.sh#L371) и
[публикация:385](../sdata/lib/uninstall.sh#L385).

Python читает весь `.bashrc`/profile как UTF-8. Его exit status не проверяется,
а перенаправление stdout заранее создаёт пустой temporary file. После
UnicodeDecodeError `cmp` обнаруживает различие и `mv` заменяет исходный profile
пустым файлом. Комментарии Bash могут содержать такие байты.

Нативное воспроизведение: `.bashrc` содержит Latin-1 комментарий и
`export IMPORTANT=yes`. После cleanup 32 исходных байта превращаются в 0;
функция возвращает 0. Ошибки I/O не внедрялись.

Исправление: сохранять байты вне managed blocks; заменять profile только после
успешного преобразования. При любой ошибке оставлять оригинал.

### SR04 · P1 · Свежая systemd Gentoo-сессия может остаться без аудио

Место: [2.setups.sh:48](../sdata/subcmd-install/2.setups.sh#L48),
конфигурация сервисов до строки 142.

Gentoo устанавливает PipeWire/WirePlumber с systemd USE, но installer не
включает `pipewire.socket`, `pipewire-pulse.socket`, `wireplumber.service`.
Такой логики нет в setup, sdata или launcher. Gentoo ebuild
`/var/db/repos/gentoo/media-video/pipewire/pipewire-1.6.8.ebuild:515` явно
предписывает ручное включение этих user units; автоматический preset не сделан.

Это дефект интеграции свежей systemd установки без уже настроенного audio
stack. Live systemd-сессия и установка пакетов в этом review не выполнялись.

Исправление: настроить user audio units с учётом masks и существующего audio
server; при отсутствии user bus обеспечить применение после входа в сессию.

### SR05 · P2 · Update сообщает успех после отказа обязательного runtime

Места: [setup:2879](../setup#L2879), [setup:2891](../setup#L2891),
[setup:2959](../setup#L2959), [success:2963](../setup#L2963).

Оба post-update dependency-router вызова не проверяют возвращённый статус и
печатают `Dependencies installed`. Далее direct вызовы
`install-python-packages` и `ensure-ytmusic-js-runtime` также не проверяются;
затем update записывает `success`. Errexit/ERR trap на этом пути нет.

Воспроизведён точный Python/runtime фрагмент update с настоящей функцией
`install-python-packages`: отсутствие uv даёт ошибку обязательных dependencies,
но итоговый exit 0 и `reported_status=success`. Только проверка наличия uv и
последующий YT helper заменены заглушками; Python install не выполнялся.

Исправление: передавать ошибки обязательных стадий в общий update status и exit;
повторять dependency check после установки, прежде чем сообщать успех.

### SR06 · P2 · Doctor не ремонтирует Quickshell без screencopy

Место: [Gentoo USE profile:44](../sdata/dist-gentoo/install-deps.sh#L44).

USE-профиль и import-only Doctor probe не учитывают `screencopy`.
[Overview.qml:1533](../modules/overview/Overview.qml#L1533) объявляет
`OverviewWidget`, который через `OverviewWindow` требует
[ScreencopyView:96](../modules/overview/OverviewWindow.qml#L96), даже когда
для Niri выбирается другая component branch.

Изолированная настоящая QML-проба импортирует QtQuick, Quickshell,
Quickshell.Wayland и создаёт `ScreencopyView`: exit 255,
`ScreencopyView is not a type`. GURU ebuild переводит эффективный `screencopy`
USE в SCREENCOPY build option. Флаг по умолчанию включён в ebuild: дефект
относится к ремонту частичной сборки `quickshell[-screencopy]`, а не ко всем
fresh installs.

Исправление: явно включить необходимый USE и проверять required QML types,
а не только доступность импортов.

### SR07 · P2 · Пользовательское управление пакетами не перенесено на Gentoo

Места: [PackageSearch.qml:52](../services/deferred/PackageSearch.qml#L52),
[install/update:65](../services/deferred/PackageSearch.qml#L65),
[search:118](../services/deferred/PackageSearch.qml#L118),
[AppCatalog.qml:102](../services/AppCatalog.qml#L102),
[defaults/config.json:355](../defaults/config.json#L355).

PackageSearch вызывает только pacman/yay/paru. Эти функции доступны из
GlobalActions и ActionModeView без distro guard. Gentoo atom `app-misc/jq`
отклоняется validation regex; имя `jq` принимается и запускает pacman.
AppCatalog определяет только pacman/apt/dnf, поэтому native Portage target
недоступен. Штатная команда updates — `kitty -e arch-update`;
потребители Island/Waffle выполняют её напрямую.

Проба использует точные JS function bodies, заменяя только terminal execution:
`gentooAtomAccepted=false`, `simpleNameAccepted=true`, emitted install command
`sudo pacman -S -- "$1"`, update command содержит `sudo pacman -Syu`.
Пакетные команды не исполнялись.

Исправление: добавить согласованный Portage backend и Gentoo defaults;
не показывать неподдерживаемые package actions как работающие.

### SR08 · P2 · Восстановление legacy runtime копирует файлы не туда

Места: [user-modifications.sh:265](../sdata/lib/user-modifications.sh#L265),
[Restore All:2070](../setup#L2070), [Restore Single:2139](../setup#L2139).

Legacy update сохраняет полный runtime в `<backup>/legacy-runtime/` и
записывает `full_runtime_copy=true`. UI restore игнорирует metadata и сохраняет
этот prefix при копировании. Пользовательская QML попадает в
`<runtime>/legacy-runtime/shell.qml`, а действующий `<runtime>/shell.qml`
остаётся shipped.

Настоящий rsync preserve и точная restore function в temporary fixture:
exit 0, активный shell остался `// shipped`, пользовательская версия оказалась
в лишнем подкаталоге. Сохранённый backup существует, поэтому это P2.

Исправление: выбирать source root по `full_runtime_copy`/label metadata для
обоих restore режимов.

### SR09 · P2 · Релизная проверка не проверяет реальную оболочку и часть регрессий

Места: [release.sh:298](../scripts/release.sh#L298),
[Makefile:26](../Makefile#L26).

Release gate запускает только `make test-local`. Native shell startup и
audit suite не входят в обязательный publish check. Даже отдельный
`make test-audit` не запускает `test-review-regressions.py` с 36 дополнительными
проверками migration/config/rollback/startup. Поэтому `ready to publish` не
подтверждает эти контракты или загрузку entrypoint.

Исправление: объединить необходимые acceptance suites в обязательной релизной
проверке и выполнять её из чистого revision с подготовленными Gentoo deps.

## Свежие проверки и границы доказательств

| Проверка | Результат |
|---|---|
| `python3 scripts/test-review-regressions.py` | PASS, 50 tests, включая fault injection SR01–SR09 и последующие фиксы |
| `python3 scripts/test-audit-regressions.py` | PASS, 21 tests; Gentoo/service calls заглушены |
| `make test-local` | PASS |
| `INIR_REQUIRE_NATIVE_STARTUP=1 python3 scripts/test-shell-startup.py` | FAIL, 13 native startup cases из-за отсутствующих Quickshell/Qt modules; release gate теперь блокирует публикацию |
| `make test-audit` | Не проходит из-за той же недоступной native runtime; audit/review suites проходят |

Root shell в этой установленной среде всё ещё не загружается ни в одном из
трёх семейств из-за отсутствующих модулей Quickshell/Qt. Это подтверждённый
блокер проверки среды; успешность оболочки после правильной Portage сборки
не установлена. Не проводились реальные emerge, reboot/logout,
сервисные переключения или hardware/PAM/screencast проверки.

`git ls-files` по-прежнему возвращает только LICENSE. Проверенное дерево нельзя
воспроизвести checkout текущего commit. Проверка untracked в release code
уже исправлена, но состояние revision остаётся блокером публикации.

## Дополнительные замечания повторной проверки

Повторный проход выявил четыре дефекта в уже исправленных путях: rollback
миграции терял ссылку на внешний config; Restore All/Single игнорировали ошибки
копирования; Gentoo-каталог содержал недоступные или неверно категоризированные
atoms; release gate разрешал пропустить native startup при отсутствующих
предусловиях. Они исправлены, проверки добавлены в regression suite. В restore
копия публикуется атомарно, при миграции восстанавливается разрешённый target и
исходный текст symlink, а пользовательская ссылка при restore сохраняется.

## Статус исправлений

После review исправлены все SR01–SR09: uninstall и migration теперь требуют
проверенный backup; очистка profile сохраняет произвольные байты; Gentoo
systemd получает пользовательские PipeWire/WirePlumber units; Doctor проверяет
`ScreencopyView`; update корректно сообщает об ошибках обязательных runtime стадий; package
actions и каталог получили Portage backend; оба restore режима раскрывают
payload root и возвращают ошибки копирования; release gate включает review suite
и требует native shell startup, включая его предусловия.

В этом окружении Quickshell modules отсутствуют, поэтому загрузку оболочки
подтвердить не удалось. Live Gentoo systemd/Portage проверка также не
выполнялась.

Release gate по-прежнему требует чистый tracked revision. В этом checkout
`git ls-files` содержит только LICENSE, а файлы проекта untracked; публикацию
нельзя подтвердить до восстановления корректной истории репозитория.
