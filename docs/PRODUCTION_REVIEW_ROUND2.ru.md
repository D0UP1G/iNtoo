# Production review iNtoo: повторный аудит

Дата: 2026-10-07. Проверено рабочее дерево в Gentoo, исходный Git HEAD — `19fcad0`.

**Обновление после исправлений:** PR05–PR16 исправлены в рабочем дереве и покрыты
регрессиями. Текущие результаты и оставшиеся ограничения выпуска приведены в
[отчёте об исправлениях](PRODUCTION_FIXES_ROUND2.ru.md). Ниже сохранён исходный
аудит состояния до этих изменений.

**Решение: NEEDS CHANGES. Текущее состояние к production не готово.**
Найдены 12 дополнительных замечаний: четыре P1 и восемь P2. Наиболее серьёзные
касаются удаления чужих данных, перезаписи конфигурации без backup и состояния
незавершённого обновления. Два ранее известных ограничения выпуска также остаются:
native QML startup не проходит, а Git revision содержит только `LICENSE`.

Аудит не изменял runtime-код. Воспроизведения удаления, установки, обновления и
rollback выполнялись во временных каталогах. Вызовы реальных сервисов, package
merge/unmerge, shutdown и reboot не выполнялись. Ниже отдельно указаны проверки
с подменёнными внешними командами и native проверки.

## Новые замечания

### PR05 · P1 · Uninstall удаляет соседний каталог при метасимволах в XDG-пути

**Место:** [sdata/lib/uninstall.sh](../sdata/lib/uninstall.sh), строки 523, 533, 538.

Ключи `INIR_ONLY_PATHS` уже содержат раскрытые абсолютные пути. Повторный
`eval echo "$path"` выполняет glob expansion. При
`XDG_CONFIG_HOME=/tmp/fixture/config[1]` и существующем соседнем
`/tmp/fixture/config1/inir` удаление выбирает соседний каталог.

Воспроизведение исполняет настоящие `uninstall_create_backup` и
`uninstall_remove_inir_only`. В обоих каталогах создан файл с пользовательскими
данными. Backup сохраняет правильный каталог, но удаляется другой:

~~~text
backup_of_intended_exists=yes
backup_of_neighbor_exists=no
process_rc: 0
intended directory remains: True
neighbor unrelated data remains: False
~~~

**Последствие:** данные вне выбранной конфигурации удалены без соответствующего
backup; intended каталог остаётся. Это воспроизведённая потеря данных в fixture.

**Исправление:** использовать путь буквально, убрать `eval` из операций над уже
раскрытыми путями; перед удалением проверять границы выбранного каталога.
Регрессия должна проверять `[]`, пробелы и сохранность соседних данных.

### PR06 · P1 · First-run install перезаписывает Niri config после пропущенного backup

**Место:** [sdata/subcmd-install/3.files.sh](../sdata/subcmd-install/3.files.sh),
строки 86, 91, 247–254; [sdata/lib/functions.sh](../sdata/lib/functions.sh),
функция `backup_clashing_targets`.

В вызове `backup_clashing_targets` переменная `$XDG_CONFIG_HOME` не заключена в
кавычки. Пробел разбивает путь на аргументы, смещает target и backup, и существующие
конфиги не обнаруживаются. Статус `auto_backup_configs` не останавливает установку.
First-run ветка затем синхронизирует defaults поверх существующего Niri config,
исходя из предположения, что backup уже сохранён.

В fixture с `XDG_CONFIG_HOME=".../config with space"` запущены настоящие
`auto_backup_configs` и `install_dir__sync "defaults/niri" ...`, включая реальный
payload filter и rsync:

~~~text
backup_rc:1 backup_exists:no
install_dir_rc:0
installed_content:new config
~~~

**Последствие:** исходный `config.kdl` заменён, backup отсутствует.
Условие — первая установка iNtoo поверх ранее настроенного Niri. Ветка повторной
установки, сохраняющая существующий Niri config, этим probe не проверялась.

**Исправление:** заключить пути в кавычки, явно возвращать ошибку backup и
прекращать запись поверх существующих конфигов при отказе сохранения.
Добавить first-run регрессию с пробелом в XDG-пути и ошибкой rsync.

### PR07 · P1 · Повторный update не завершает ранее проваленный обязательный шаг

**Место:** [setup](../setup), строки 2626–2643 и 2784; обязательный YouTube Music
runtime — строки 3035–3038.

`set_installed_version` публикует новый installed commit сразу после синхронизации
файлов, до обязательных runtime-шагов и рестарта. Если последующий шаг завершается
ошибкой, повторный update видит равные installed/repo commits, пишет `success` и
возвращается из ветки «Already up to date».

В probe выполнена настоящая функция `run_update` с реальным version tracking
в temp XDG. Сетевые, сервисные и установочные операции подменены; обязательный
`ensure-ytmusic-js-runtime` возвращает отказ:

~~~text
YT_RUNTIME_CALLED
first_rc:1 status:failed:37:YouTube Music runtime setup failed installed:19fcad0
SUCCESS:Already up to date (2.32.0)
retry_rc:0 status:success
~~~

Runtime helper был вызван только один раз за два запуска. Pending migrations
в ветке «Already up to date» обрабатываются; Python/YouTube runtime и проверка
рестарта повторно не выполняются.

**Последствие:** штатный retry превращает незавершённое обновление в успешное,
не устранив отказ обязательного шага. Эта же ранняя запись commit затрагивает
другие ошибки после синхронизации.

**Исправление:** публиковать completion revision после обязательных шагов либо
хранить отдельное состояние incomplete update и возобновлять необходимые этапы.
Регрессия должна выполнять два последовательных update после отказа runtime
и после отказа загрузки shell.

### PR08 · P1 · Bootstrap GURU не устанавливает отсутствующий модуль eselect repository

**Место:** [sdata/dist-gentoo/install-deps.sh](../sdata/dist-gentoo/install-deps.sh),
строки 104–112, 249, 255.

Проверка `command -v eselect` не подтверждает наличие модуля `repository`.
Gentoo может иметь `app-admin/eselect` без отдельного
`app-eselect/eselect-repository`. Тогда установка модуля пропускается,
`eselect repository enable guru` падает, а core dependencies, включающие
нужный пакет, ещё не устанавливались.

Настоящий `gentoo_prepare_guru` выполнен с eselect fixture, имитирующим
отсутствующий модуль. `gentoo_emerge` регистрирует вызовы без установки:

~~~text
eselect repository enable guru
Error: Cannot load module repository
RESULT=1
~~~

Вызов установки модуля отсутствует. Раздельная поставка eselect и модуля
подтверждена локальными Gentoo ebuilds
`app-admin/eselect/eselect-1.4.32.ebuild` и
`app-eselect/eselect-repository/eselect-repository-15.ebuild`.

**Последствие:** автоматическая установка останавливается до подготовки GURU
на системе без repository module.

**Исправление:** проверять работоспособность модуля и устанавливать его до
первого обращения к `eselect repository`. Нужна регрессия «eselect есть,
repository module отсутствует».

### PR09 · P2 · Отказ systemctl restart классифицируется как успешный update

**Место:** [sdata/lib/robust-update.sh](../sdata/lib/robust-update.sh), строка 156;
[setup](../setup), строки 3055 и 3077.

Явный ненулевой ответ `systemctl --user restart inir.service` преобразуется
в код 2, предназначенный для неподтверждённого результата. Caller считает эту
ветку «restart requested», после чего записывает `success`.

Настоящий restart helper и завершающий update flow с подменённым
`systemctl restart`, возвращающим 1:

~~~text
verify_rc:2
phase:Shell restart requested; 'inir logs' shows how it went
flow_rc:0 status:success
~~~

**Последствие:** отказ задания рестарта скрыт успешным статусом update.
В реальной системе такой отказ может сопровождаться остановленным shell.
Сервисы в probe не переключались.

**Исправление:** различать отказ команды рестарта и отсутствие подтверждения
по журналу. Отказ команды должен доходить до failure branch caller.
Существующая PR02 регрессия подменяет helper кодом 1 и этот переход не покрывает.

### PR10 · P2 · Rollback присваивает старому runtime commit нового checkout

**Место:** [sdata/lib/snapshots.sh](../sdata/lib/snapshots.sh), строки 30 и 398;
[setup](../setup), строка 2626.

Snapshot берёт `commit_before` из HEAD репозитория, а `version_before` —
из installed version. В режиме repo-copy checkout уже может содержать новый
commit, когда установленный runtime ещё старый. В snapshot попадает старый payload
с новым commit. После rollback этот commit публикуется как installed.

Probe создаёт реальный temp Git repo с commits A и B, runtime A и checkout B.
Настоящие `create_snapshot` и `restore_snapshot` дают:

~~~text
before_snapshot_installed:A head:B recorded:B
restored_payload:old payload installed_commit:B installed_version:1.0
~~~

**Последствие:** rollback возвращает файлы A, но update затем считает их
соответствующими checkout B и может пропустить синхронизацию.
Буквы A/B заменяют случайные hashes fixture.

**Исправление:** отдельно сохранять checkout revision и installed payload
revision; после восстановления runtime публиковать revision его содержимого.
Проверить snapshot/rollback/update в repo-copy при несовпадающих revisions.

### PR11 · P2 · Retention может сразу удалить только что сохранённые изменения

**Место:** [sdata/lib/user-modifications.sh](../sdata/lib/user-modifications.sh),
строки 270, 272, 340, 376, 395.

После публикации нового backup выполняется retention по filesystem mtime.
Если старые backup имеют время из будущего, например после коррекции системных
часов назад, новый каталог оказывается самым старым и удаляется. Функция всё равно
возвращает его путь и handler сообщает успешное сохранение.

В fixture созданы пять старых backup с mtime на час впереди текущего времени.
Настоящий `handle_user_modifications` сохраняет изменённый `shell.qml`:

~~~text
SUCCESS: Auto-preserved 1 file(s) to: .../backups/<new-backup>
handler_rc=0
backup_file_exists=no
~~~

Все пять старых каталогов остаются; новый удалён.

**Последствие:** обещанный backup пользовательских изменений отсутствует,
а caller может продолжить замену runtime. Наличие отдельного update snapshot
может сохранить ещё одну копию; потеря всех способов восстановления не доказана.

**Исправление:** исключать только что опубликованный backup из текущей очистки
и подтверждать его существование перед успехом. Добавить регрессию с future mtime.

### PR12 · P2 · Refresh каталога теряет источник установки перед удалением приложения

**Место:** [services/AppCatalog.qml](../services/AppCatalog.qml), строки 134,
315–342; [SoftwareView.qml](../modules/sidebarLeft/SoftwareView.qml), строка 407.

`_refreshInstalled` сразу очищает `_installedSources`, но старый
`installedPackages` остаётся до завершения процесса. Карточки в это время
позволяют удаление. Без сохранённого источника `removeApp` выбирает native backend.

Node probe исполняет настоящие JS bodies из QML и парсер установленного Firefox
Flatpak из текущего app catalog:

~~~text
before: installed=true source=flatpak
after refresh begins: installed=true sources={} checkingInstalled=true
removeApp accepted=true
command: sudo emerge --ask --unmerge "$1"
args: www-client/firefox
~~~

**Последствие:** Flatpak остаётся установленным; пользователю предлагается
удаление native Firefox. Наличие такого native пакета не требуется, чтобы
воспроизвести неверный выбор команды. Реальный unmerge не выполнялся.

**Исправление:** атомарно заменять данные установки и их источники либо
блокировать действия до завершения refresh. Регрессия PR04 должна включать
удаление во время refresh, а не только после заполнения source map.

### PR13 · P2 · Русская локаль обходит защиту активного PulseAudio

**Место:** [sdata/subcmd-install/2.setups.sh](../sdata/subcmd-install/2.setups.sh),
строки 54, 59, 96.

`pactl info` разбирается по английскому `Server Name:` без `LC_ALL=C`.
В русской локали ключ — `Имя сервера:`, поэтому результат пустой. Fallback
на `pgrep pulseaudio` находится в `elif` и не выполняется, если pactl существует.

Настоящий `setup_systemd_audio_services` с подменёнными pactl/systemctl:
английский вывод сохраняет активный PulseAudio и не вызывает enable;
русский вызывает `enable --now` для `pipewire.socket`,
`pipewire-pulse.socket` и `wireplumber.service`.
Перевод ключа проверен в установленном `pulseaudio.mo`.

**Последствие:** установка пытается включить конкурирующий аудиостек при уже
активном PulseAudio. Реальные аудиосервисы в probe не менялись; фактическая потеря
звука не проверялась.

**Исправление:** использовать стабильную локаль для разбора и выполнять fallback
при недоступном или нераспознанном ответе. Проверить RU locale и отказ pactl.

### PR14 · P2 · Cleanup сигналит thumbnail jobs без проверки владельца

**Место:** [scripts/inir](../scripts/inir), строки 2231–2243;
[assets/systemd/inir.service](../assets/systemd/inir.service), строка 58.

Phase 6 выбирает процессы глобальным `pgrep -f scripts/thumbnails/...` и
посылает TERM, затем KILL. В отличие от соседних фаз, service/config ownership
не проверяется. `ExecStopPost` вызывает эту очистку при остановке сервиса.

Настоящее тело `cleanup_orphans` выполнено с temp runtime path, подменёнными
pgrep и kill. Процесс fixture без связи с сервисом всё равно выбран:

~~~text
would-kill 999999
would-kill -0 999999
would-kill -KILL 999999
~~~

**Последствие:** stop/restart может прервать самостоятельную генерацию thumbnail
или job другой конфигурации/checkout того же пользователя. Реальные сигналы
в probe не посылались.

**Исправление:** отслеживать принадлежность jobs текущему shell/config, включая
отдельные scopes. Простая проверка только `inir.service` недостаточна:
[thumbgen-venv.sh](../scripts/thumbnails/thumbgen-venv.sh), строки 13–22,
намеренно запускает штатный batch в отдельном scope.

### PR15 · P2 · Manual Gentoo USE snippet не включает требуемый screencopy

**Место:** [docs/GENTOO.md](GENTOO.md), строка 47;
[OverviewWindow.qml](../modules/overview/OverviewWindow.qml), строка 96.

В ручной инструкции отсутствует `screencopy`, хотя Hyprland-компонент
`OverviewWindow` создаёт `ScreencopyView` без условного loader. Для Niri
выбирается другой overview-компонент; полный отказ Niri overview этим сравнением
не доказан. Автоматическая настройка USE
в [install-deps.sh](../sdata/dist-gentoo/install-deps.sh), строка 44,
положительный флаг уже содержит.

Локальный GURU ebuild `quickshell-0.3.1.ebuild` передаёт этот USE в
`-DSCREENCOPY`. По умолчанию флаг включён, поэтому это не универсальный отказ
fresh install. Однако ранее заданный `-screencopy` ручная инструкция не отменяет,
и компонент остаётся недоступным.

**Последствие:** инструкция восстановления зависимостей неполна для системы
с явно отключённым screencopy. Сопоставление docs, consumer и ebuild подтверждено;
полный startup после пересборки Quickshell не выполнялся.

**Исправление:** синхронизировать ручной USE snippet с требованиями runtime
и проверять доступность типа `ScreencopyView`, а не только QML imports.

### PR16 · P2 · Session warning не обнаруживает Portage

**Место:** [SessionWarnings.qml](../services/deferred/SessionWarnings.qml),
строка 28; [SessionScreen.qml](../modules/sessionScreen/SessionScreen.qml),
строки 401, 411, 443; [session-power.sh](../scripts/session-power.sh), строка 7.

Список package manager processes не содержит Portage/emerge. Поэтому Gentoo
операции не включают предусмотренное предупреждение перед power action.
Power helper также передаёт `-i`, игнорируя inhibitors.

Проверен настоящий read-only `emerge --search firefox`: процесс имеет
`comm=emerge` и Python cmdline. Текущий detector возвращает 1.
Обычный `pidof emerge` тоже возвращает 1; `pidof -x emerge` обнаруживает
именно этот процесс. Отдельный process fixture также показывает отсутствие
emerge в текущем списке.

**Последствие:** предупреждение не появляется при активности целевого package
manager. Прерывание настоящего package merge и shutdown не проверялись.
Проверка search подтверждает распознавание процесса; различать search и merge
будущему detector ещё потребуется.

**Исправление:** добавить корректное обнаружение активной Portage операции
с учётом Python entrypoint и фактической операции. Одного добавления
`emerge` в существующий `pidof` список недостаточно.

## Сохранившиеся ограничения выпуска

1. **Native startup не проходит в текущем Gentoo runtime.** `make test-audit`
   даёт 13 failures: десять отсутствующих импортов и отказ загрузки каждого
   из трёх семейств ii, Iris, Waffle. Отсутствуют `QtMultimedia`,
   `Quickshell.Bluetooth`, `Quickshell.Hyprland`, `Quickshell.Networking`,
   `Quickshell.Services.Mpris`, `Notifications`, `Pam`, `Pipewire`,
   `SystemTray` и `UPower`. Это реальные offscreen QML запуски в изоляции;
   Wayland rendering и работа desktop-сессии ими не подтверждаются.
   До подготовки зависимостей нельзя проверить последующие runtime-сценарии.
2. **Проверенное дерево не воспроизводится из Git revision.** `git ls-files`
   возвращает только `LICENSE`; 39 top-level записей имеют статус untracked.
   HEAD `19fcad0` не содержит проверенный проект. Нужен полный reviewable tracked
   revision для release. Staging и commits в этом аудите не выполнялись.

## Выполненные проверки и границы доказательств

| Проверка | Результат |
|---|---|
| `bash scripts/test-local-distribution.sh` | PASS, exit 0; включая 10 payload и 34 OpenRC checks |
| `make test-audit` | FAIL, exit 2; 21 audit и 56 review tests PASS; native startup: 7 test methods, 13 failures |
| Изолированные user-data probes | 2 сценария воспроизведены: uninstall и retention |
| Изолированные lifecycle probes | 4 сценария воспроизведены, exit 0 |
| Gentoo probes | bootstrap/module, audio locale и manual USE mismatch подтверждены |
| Runtime probes | неверный remove backend и cleanup signal selection подтверждены; Portage detector отдельно проверен read-only |

Local suite и audit suite запущены заново для этого review. Их успех не покрывает
описанные дополнительные состояния. Аудит отдельно проверил реальные тела Bash
functions и JS functions из QML, вместо заключения только по textual assertions.
В JS probes поведение Quickshell Process/UI callbacks моделируется; это не
проверка взаимодействия в живой desktop-сессии.

Артефакты текущей сессии находятся вне репозитория и могут удаляться вместе с
`/tmp`:

- local suite: `/tmp/intoo-local-round2.FAzc2a`;
- audit suite: `/tmp/intoo-audit-round2.G8K8gM`;
- probes: `/tmp/intoo-audit-userdata-probes.py`,
  `/tmp/intoo-audit-lifecycle-probes.py`,
  `/tmp/intoo-audit-gentoo-probes.py`,
  `/tmp/intoo-audit-runtime-probes.py`.

Перед выпуском необходимо исправить P1, разобрать P2, добавить регрессии для
указанных состояний и получить успешный native startup на полном revision.
После этого нужны проверки установки, update/retry, rollback и uninstall
на disposable Gentoo системах с systemd и OpenRC, а также реальная Wayland-сессия.
Mocked сервисные проверки не заменяют эти системные проверки.
