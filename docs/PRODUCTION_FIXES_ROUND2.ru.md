# Исправления production audit PR05–PR16

Дата: 2026-10-07. Исправлены 12 замечаний из
[повторного аудита](PRODUCTION_REVIEW_ROUND2.ru.md).
Исправления внесены в рабочее дерево; установка на рабочий desktop не выполнялась.

## Что изменено

| Замечание | Исправление |
|---|---|
| PR05 · uninstall | Пути из всех трёх списков удаления используются буквально, без `eval`. Пробелы, `[]` и `$()` не раскрываются повторно. Удаление поддерживает dangling symlinks и передаёт `--` перед путями. |
| PR06 · backup перед install | XDG-путь заключён в кавычки. Имена конфликтующих файлов сохраняются без word splitting и rsync glob filters. Backup сначала собирается в staging; ошибка прекращает установку и удаляет staging. Готовый backup публикуется rename; существующая копия сохраняется. |
| PR07 · update/retry | Completion revision записывается после обязательного Python/YT runtime и проверки рестарта. Перед синхронизацией сохраняется `update-incomplete`; marker остаётся при отказе и не даёт retry выйти из ветки «Already up to date», включая repo-link. Для repo-link сравнивается сохранённый completion commit, а не live HEAD. Старые `failed:*`/`progress:*` statuses тоже заставляют повторить этапы. Ошибки записи version metadata распространяются caller-у. |
| PR08 · GURU | Проверяется работоспособность `eselect repository`, а не только наличие eselect. Модуль устанавливается до enable; повторная проверка после установки обязательна. |
| PR09 · restart | Отказ `systemctl restart` возвращает код подтверждённой ошибки 1. Код 2 остаётся для результата, который нельзя подтвердить по журналу. |
| PR10 · rollback | Snapshot отдельно хранит checkout commit и `installed_commit_before`. Rollback публикует revision сохранённого payload. После interrupted update revision снимка считается `unknown`. Legacy snapshots берут commit из payload metadata либо используют `unknown`; corrupt metadata отклоняется до остановки shell. |
| PR11 · retention | Только что опубликованный backup исключён из текущего pruning. Future mtime старых копий больше не приводит к немедленному удалению новой. Перед успешным возвратом проверяется наличие backup. |
| PR12 · каталог приложений | Refresh сохраняет предыдущий source map до публикации результата. `removeApp` и карточки отключены во время проверки, поэтому удаление Flatpak не перенаправляется в Portage. |
| PR13 · PulseAudio | pactl разбирается с `LC_ALL=C`; при пустом/неудачном ответе работает process fallback. Активный PulseAudio сохраняется. |
| PR14 · thumbnail jobs | Cleanup проверяет UID, cgroup и config-specific scope. Штатный service wrapper присваивает batch scope имя с digest пути конфигурации. Перед KILL повторно проверяются ownership и start time процесса. Jobs вне service/scope ownership сохраняются. OpenRC wrapper не использует systemd-run только из-за наличия бинарника. |
| PR15 · Gentoo docs | Ручной USE snippet содержит `screencopy` и совпадает с автоматическим набором runtime flags. |
| PR16 · Portage warning | Новый helper читает argv из `/proc`, распознаёт Python entrypoint emerge и предупреждает об операциях, изменяющих пакеты. Search/info/help/pretend не считаются merge. Остальные package managers сохранены в detector. |

## Регрессии

Добавлен [scripts/test-production-round2.py](../scripts/test-production-round2.py)
и подключён в `make test-audit`. Итоговая отдельная проверка: **27 tests PASS**.
Проверяются настоящие тела lifecycle functions и JS functions из QML.

Покрыты:

- сохранность соседних данных при uninstall; буквальные пробелы, brackets и `$()`;
- реальный rsync backup перед установкой Niri defaults, refusal при отказе backup,
  повтор после отказа, dotfiles, специальные имена и ignored files;
- два update подряд после отказа Python runtime, YT runtime и загрузки shell;
  равный commit, repo-link retry и отказ version metadata;
- eselect с отсутствующим repository module и отказ bootstrap;
- отказ systemd restart и отказ OpenRC launcher;
- rollback при различающихся checkout/payload revisions, legacy metadata,
  interrupted update и corrupt snapshot;
- partial/full backup при future mtime;
- Flatpak remove во время refresh;
- RU locale и отказ pactl;
- чужие/свои thumbnail scopes, service wrapper, PID reuse и OpenRC dispatch;
- соответствие manual/automatic USE и Portage detection с Python argv.

Все lifecycle записи выполняются в temporary XDG/Git fixtures. Реальные package
merge/unmerge, переключение сервисов и power actions не выполнялись. Сигналы
cleanup регистрируются stub-ом; tests завершают только свои sleep subprocesses.
JS probes моделируют Process/UI callbacks и не заменяют desktop-проверку.

## Проверки

| Команда | Результат |
|---|---|
| `python3 scripts/test-production-round2.py` | PASS, 27 tests |
| `python3 scripts/test-review-regressions.py` | PASS, 56 tests |
| `python3 scripts/test-audit-regressions.py` | PASS, 21 tests |
| `bash scripts/test-local-distribution.sh` | PASS; включая 10 payload и 34 OpenRC tests |
| `make test-audit` | FAIL: 21 audit, 56 review и 26 round2 tests PASS; native startup — 13 failures. После этого отдельный round2 suite с дополнительной repo-link регрессией: 27 PASS |
| `bash -n` для всех изменённых shell files | PASS |
| ShellCheck 0.11.0 с `.shellcheckrc` | Новых категорий diagnostics относительно исходных файлов нет; полный lint не чист из-за существующих замечаний |

ShellCheck выполнялся с `--extended-analysis=false`: стандартный запуск был
завершён системой сигналом KILL; повторный общий запуск с отключённым dataflow
анализом превысил timeout 60 секунд. В исходном коде и итоговом lint остаются, в
частности, SC2168 для `local` в файле, который installer source-ит из функции.
Это не результат успешного полного dataflow анализа.
Итоговый запуск по одному файлу завершён: 83 существующих diagnostics, новых
категорий diagnostics относительно исходных файлов нет.

## Что ещё не подтверждено для выпуска

**Production readiness остаётся неподтверждённой.** Native startup на текущем
Gentoo runtime продолжает падать из-за отсутствующих девяти Quickshell modules
и `QtMultimedia`; ii, Iris и Waffle пока не загружаются. Изменения installer и
manual dependency instructions подготавливают нужные flags/packages, но не
пересобирают текущие системные пакеты автоматически в ходе source-level fix.

`git ls-files` по-прежнему возвращает только `LICENSE`. Полный tracked revision
не создан; staging/commit/release не выполнялись.

После подготовки зависимостей и полного revision нужны успешный native startup,
реальная Wayland-сессия и lifecycle проверки на disposable Gentoo системах с
systemd/OpenRC. Mocked сервисные проверки не подтверждают системное поведение.
