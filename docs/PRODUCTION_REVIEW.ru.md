# Production review iNtoo

Дата: 2026-10-07. Результаты проверки после исправлений PR01–PR04.

**Решение: NEEDS CHANGES. В production выпускать нельзя.**
[Последующий аудит](PRODUCTION_REVIEW_ROUND2.ru.md) выявил ещё 12 замечаний,
включая потерю данных в uninstall и first-run install и некорректный retry update.
Ниже сохранены результаты исправлений PR01–PR04. Два ограничения выпуска —
отсутствующие native runtime модули и неполный Git revision — также остаются.

PR05–PR16 из последующего аудита теперь также исправлены:
[результаты и регрессии](PRODUCTION_FIXES_ROUND2.ru.md).

## Исправленные дефекты

1. **PR01 · P1 · Очистка backup могла удалить посторонние данные.**

   В [user-modifications.sh](../sdata/lib/user-modifications.sh) очистка теперь
   канонизирует корень backup, сортирует записи с NUL-разделителем, хранит пути в
   Bash-массиве и перед `rm` проверяет родительский каталог. Пробелы в пути не
   разбивают аргументы удаления.

   Регрессия создаёт `USER_MODS_DIR` с пробелом, шесть backup-каталогов и
   посторонний файл в похожем относительном пути. Проверено: удаляется только
   старый backup, посторонний файл остаётся.

2. **PR02 · P1 · Update сообщал успех при подтверждённом отказе shell.**

   В [setup](../setup) проверенный отказ `restart_shell_and_verify` теперь
   записывает `failed:43`, возвращает ненулевой код и не показывает completion.
   Статус `success` записывается после успешных обязательных шагов и проверки
   рестарта. Состояние «рестарт не удалось подтвердить» остаётся отдельной веткой.

   Регрессия исполняет завершающий flow update с подменённым отказом рестарта и
   проверяет статус, exit code и отсутствие completion.

3. **PR03 · P2 · Ошибки резервирования пользовательских файлов скрывались.**

   В [user-modifications.sh](../sdata/lib/user-modifications.sh) частичный и
   полный backup сначала записываются в staging-каталог. Ошибки `cp`, `rsync`,
   подсчёта файлов или metadata завершают операцию ошибкой и удаляют незаконченный
   staging-каталог; готовый backup публикуется атомарным rename. Handler и legacy
   caller в [setup](../setup) прекращают update при ошибке сохранения.

   Регрессии проверяют отказ `cp`, отказ `rsync`, отсутствие частичного backup и
   корректные файлы и JSON metadata для успешных режимов.

4. **PR04 · P2 · Steam, установленный Flatpak на Gentoo, не удалялся из каталога.**

   [AppCatalog.qml](../services/AppCatalog.qml) теперь сохраняет источник каждой
   найденной установки: native package manager или Flatpak. Удаление выбирает
   тот же источник, включая Gentoo + Flatpak fallback. [SoftwareView.qml](../modules/sidebarLeft/SoftwareView.qml)
   показывает уведомление, если способ удаления определить не удалось.

   Node-регрессия проверяет для записи Steam команду
   `flatpak uninstall -y com.valvesoftware.Steam`; UI-проверка подтверждает показ
   уведомления при отказе.

## Стоп-факторы выпуска

### Gentoo native startup

На текущей Gentoo-системе `make test-audit` подтверждает, что все три семейства
shell (ii, Iris и Waffle) не загружаются. Первая ошибка —
`Quickshell.Hyprland is not installed`; всего отсутствуют десять проверяемых
импортов: девять `Quickshell.*` модулей и `QtMultimedia`.

Read-only `emerge -pv gui-apps/quickshell::guru dev-qt/qtmultimedia` показал, что
Quickshell ожидает пересборки с заданными USE-флагами, а `dev-qt/qtmultimedia`
ещё не установлен. Это объясняет состояние текущего runtime, но не заменяет
проверку после установки зависимостей. Системные пакеты и пользовательская сессия
в этой работе не изменялись. До установки и успешной native-загрузки всех семейств
production readiness не подтверждён.

### Git revision

`git ls-files` возвращает только `LICENSE`; остальные файлы проекта untracked.
Поэтому проверенное состояние нельзя получить checkout-ом текущего revision, а
release gate корректно отклоняет рабочее дерево. До формирования полного tracked
revision поставку принимать нельзя. Файлы не staged и commit не создавался.

## Проверки

| Проверка | Результат |
|---|---|
| `make test-local` | PASS; включая 10 payload и 34 OpenRC проверки |
| `python3 scripts/test-review-regressions.py` | PASS, 56 тестов |
| `python3 scripts/test-runtime-payload.py` | PASS, 10 тестов |
| `node scripts/test-idle-policy.js` | PASS |
| `bash -n setup sdata/lib/user-modifications.sh` | PASS |
| `make test-audit` | FAIL: 21 audit и 56 review тестов проходят; native startup имеет 13 failures, вызванных отсутствующими импортами |

Реальные emerge, update/uninstall, переключение сервисов, reboot/logout и
hardware/PAM/lock сценарии не запускались. Установка пакетов в текущей Gentoo
системе и публикация revision остаются обязательными перед production.
