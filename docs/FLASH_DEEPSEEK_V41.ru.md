# План разработки «Flash DeepSeek v4.1»

> Подробный план доработки iNtoo: исправление assistant, выбор провайдера и API
> по умолчанию, ускорение загрузки без роста потребления RAM и экономия оперативной
> памяти.
>
> Базовый checkout: `main` @ `56dd3de` (VERSION 2.32.2). Целевая ветка: `prerelease`.

## Содержание

1. [Резюме](#резюме)
2. [Текущее состояние assistant](#текущее-состояние-assistant)
3. [Часть 1. Assistant: выбор провайдера и API по умолчанию + исправления](#часть-1-assistant-выбор-провайдера-и-api-по-умолчанию--исправления)
4. [Часть 2. Ускорение загрузки без роста RAM](#часть-2-ускорение-загрузки-без-роста-ram)
5. [Часть 3. Экономия оперативной памяти](#часть-3-экономия-оперативной-памяти)
6. [Порядок работ, коммиты и PR](#порядок-работ-коммиты-и-pr)
7. [Метрики и проверка результата](#метрики-и-проверка-результата)
8. [Риски и открытые вопросы](#риски-и-открытые-вопросы)

---

## Резюме

Три цели:

1. **Assistant.** В Settings → AI и в чате нет явного выбора «провайдер и API по
   умолчанию»: выбор модели при старте происходит эвристически
   (`recommendedModelIds("auto")`), а провайдер/формат API выводятся из выбранной
   модели. Добавляем конфигурируемый провайдер по умолчанию (+ опционально
   закреплённую модель и формат API), резолвер по умолчанию в `services/Ai.qml`,
   UI в обеих семьях (ii и Waffle), пресет DeepSeek и набор исправлений найденных
   дефектов.
2. **Загрузка.** Убираем оставшиеся узкие места старта, не увеличивая RSS:
   перенос/растягивание работы, ленивая материализация сервисов, отложенные
   `Process`-вызовы и т.п.
3. **RAM.** Ограничиваем неограниченный рост коллекций, освобождаем кэши и
   невидимые поверхности, убираем повторное создание объектов.

---

## Текущее состояние assistant

В проекте два assistant'а:

| Assistant | Где живёт | Провайдеры/API | Настройки |
|---|---|---|---|
| Сайдбар AI-чат (основной) | `services/Ai.qml` (1882 строки) + стратегии `services/ai/*` + каталог `AiProviderCatalog.qml` + пресеты `modules/common/AiProviderPresets.qml` | 11 пресетов: ollama, lmstudio, openrouter, google-gemini, groq, mistral, cerebras, anthropic, openai, opencode-zen, opencode-go + произвольные custom endpoints; форматы API: openai, openai-response, gemini, mistral, anthropic | `modules/settings/AiConfig.qml` (стр. настройки 24), `modules/waffle/settings/pages/WAiPage.qml`, выбор модели в `modules/sidebarLeft/aiChat/AiModelSelector.qml` |
| Mascot-компаньон (Super+S) | `services/MascotAssistant.qml` + `scripts/ai/mascot-assistant.py` (OpenCode 1.x) | любой `provider/model` из OpenCode; в репозитории конфигурируется только Ollama; ключи — глобальный `auth.json` OpenCode | `modules/settings/MascotConfig.qml` (только текстовое поле модели), `modules/waffle/settings/pages/WMascotPage.qml` |

### Как сейчас выбирается модель по умолчанию

`services/Ai.qml:753-764` (`_resolvedCurrentModelId`), по приоритету:

1. `Persistent.states.ai.model` (persisted-состояние сессии, дефолт схемы —
   устаревший `gemini-2.5-flash`, см. `modules/common/Persistent.qml:90`), если
   модель существует и запускаема;
2. `currentModelId`, если запускаема;
3. `recommendedModelIds("auto", 1)` — эвристика по профилю «Auto»
   (`Ai.qml:905-943`), учитывающая local/free/capabilities/имя;
4. первый runnable;
5. первый из списка вообще (может быть заблокирован — см. дефект A3).

**Провайдер по умолчанию нигде не задаётся**: `Config.options.ai` содержит только
`systemPrompt`, `tool`, `extraModels` (`modules/common/Config.qml:714-735`).
Формат API (api_format) и endpoint вытекают из выбранной модели, а не из явного
выбора пользователя.

### Что уже оптимизировано ранее (не переделывать)

- Коммит `cb48263 fix(performance): reduce island startup and ram use` закрыл
  часть находок [PERFORMANCE_ISLAND_AUDIT.ru.md](PERFORMANCE_ISLAND_AUDIT.ru.md):
  компиляция только выбранного семейства (`shell.qml:706-794`,
  `FamilyPanelHostLoader`), tiered-старт сервисов (`shell.qml:94-180`), мониторинг
  `MemAvailable`/RSS/PSS в `services/MemoryPressureService.qml:91-154`.
- `docs/OPTIMIZATION.md` — действующие правила производительности QML.
- AI-каталог уже кэшируется в `~/.local/state/inir/ai/model-catalog.json`
  (`services/ai/AiProviderCatalog.qml:271-278`), транскрипт mascot ограничен
  40 сообщениями (`services/MascotAssistant.qml:25`).

---

## Часть 1. Assistant: выбор провайдера и API по умолчанию + исправления

### 1.1 Схема конфигурации (Config.qml + defaults)

В `modules/common/Config.qml`, блок `property JsonObject ai` (сейчас строки
714–735), добавить (типизированно, с дефолтами):

```qml
property string defaultProvider: ""   // id пресета (AiProviderPresets) или "custom"
property string defaultModel: ""      // remote id модели внутри провайдера ("" = авто)
property string defaultApiFormat: ""  // "" = из пресета/extraModels; иначе openai|gemini|anthropic|mistral|openai-response
```

Синхронно (правило проекта «Sync rule», `ARCHITECTURE.md:159-163`):
- `defaults/config.json` → `ai.defaultProvider`, `ai.defaultModel`,
  `ai.defaultApiFormat` (пустые значения — поведение по умолчанию не меняется);
- потребители — см. 1.2;
- Settings UI в обеих семьях — см. 1.4;
- `modules/common/Persistent.qml:89-93`: заменить дефолт
  `ai.model: "gemini-2.5-flash"` на `""` (устаревший id не резолвится и только
  сбивает логирование/diagnose).

### 1.2 Резолвер модели по умолчанию в services/Ai.qml

Переработать `_resolvedCurrentModelId()` (`Ai.qml:753-764`) в явную цепочку:

1. `Persistent.states.ai.model` — если существует и `modelCanRun` (поведение
   «последний выбранный в сессии» сохраняем);
2. **новое**: если задан `Config.options.ai.defaultProvider`:
   - `defaultModel` задан → id `_catalogModelId({providerId, remoteId})` /
     custom-id, если существует и runnable;
   - иначе — лучшая runnable-модель этого провайдера по профилю `auto`
     (`_profileScore`), приоритет: локальные модели провайдера → free → остальные;
3. `recommendedModelIds("auto", 1)` (текущая эвристика как fallback);
4. первый runnable;
5. **ничего** (раньше возвращался потенциально заблокированный `modelList[0]` —
   исправление A6; UI уже умеет показывать состояние «не подключено»).

Вспомогательные функции (add-only, без изменения существующих сигнатур):

```qml
function defaultProviderModels(): var      // runnable-модели defaultProvider
function isDefaultModelId(id: string): bool
function setDefaultProvider(providerId: string, apiFormat: string): void
function clearDefaultProvider(): void
```

Важные детали реализации:

- Запись настроек — только `Config.setNestedValue("ai.defaultProvider", ...)`
  (прямое присваивание не эмитит `configChanged()`, `docs/OPTIMIZATION.md:234-247`).
- `_syncCurrentModel()` уже вызывается после `catalogUpdated` и `configChanged`
  (`Ai.qml:945-957`): после появления каталога провайдера по умолчанию модель
  переключится сама. Добавить guard, чтобы не перезаписывать осознанный выбор
  пользователя в сессии (если `Persistent.states.ai.model` установлен и runnable —
  его не трогаем).
- Для custom-провайдеров источник api_format — запись в `ai.extraModels` с
  `provider_id: "custom"`; `defaultApiFormat` хранить только при явном
  переопределении.
- Политика `policies.ai === 2` («Local only»): defaultProvider игнорируется,
  если у провайдера нет локальных моделей; не показывать ошибку, а честно
  фолбэчить (логика `modelCanRun` уже режет non-local, `Ai.qml:248-252`).

### 1.3 Пресет DeepSeek (кодовое имя плана)

Добавить в `modules/common/AiProviderPresets.qml:19-220`:

```qml
{
    "id": "deepseek",
    "name": "DeepSeek",
    "icon": "deepseek-symbolic",
    "endpoint": "https://api.deepseek.com/chat/completions",
    "model": "deepseek-chat",
    "api_format": "openai",
    "requiresKey": true,
    "keyId": "deepseek",
    "keyGetLink": "https://platform.deepseek.com/api_keys",
    "description": Translation.tr("DeepSeek chat and reasoning models over an OpenAI-compatible API."),
    "free": false,
    "local": false,
    "featured": true,
    "catalogKind": "openai-compatible",
    "modelsEndpoint": "https://api.deepseek.com/models",
    "authScheme": "bearer",
},
```

- OpenAI-совместимый формат уже полностью покрыт `OpenAiApiStrategy`
  (`services/ai/OpenAiApiStrategy.qml`); `deepseek-reasoner` даёт
  `reasoning_content`-дельты, которые парсер уже обрабатывает
  (`OpenAiApiStrategy.qml:129`). Проверить стриминг `deepseek-reasoner` вживую.
- `guessModelLogo` уже маппит `deepseek` → `deepseek-symbolic` (`Ai.qml:996`);
  проверить наличие иконки в assets.
- `discover-provider-models.py` generic-normalizer (`scripts/ai/discover-provider-models.py:326-379`)
  корректно читает `GET https://api.deepseek.com/models` (`data[].id`), изменений
  не требует; при желании добавить тест-фикстуру.

### 1.4 Settings UI: ii (`AiConfig.qml`) и Waffle (`WAiPage.qml`) — паритет

Общее для обеих страниц:

- **Секция Providers** (`AiConfig.qml:181-373`, `WAiPage.qml:143-225`):
  - бейдж «Default» на карточке провайдера по умолчанию
    (`Config.options.ai.defaultProvider === preset.id`);
  - кнопка-звезда/чекбокс «Use as default» на каждой карточке пресета и на
    custom-строках; выбор записывает `ai.defaultProvider` и
    `ai.defaultApiFormat` (= api_format пресета или записи);
  - «Clear default» при повторном нажатии.
- **Секция Setup** (`AiConfig.qml:46-166`): статусная строка «Default provider:
  X · API: OpenAI-compatible · model: Y» с кнопкой перехода в Providers.
- **Форма провайдера** (`AiConfig.qml:572-903`): при сохранении пресета
  предлагать чекбокс «Set as default»; для custom-формы выбранный
  `selectedFormat` уже является «выбором API» — связать с
  `defaultApiFormat`.
- Иконка/цвета — только через токены (`Appearance.*` / `Looks.*`), без хардкода
  (`CONTRIBUTING.md:101-129`).

### 1.5 Селектор модели в чате (`AiModelSelector.qml`)

- В `entries` (`modules/sidebarLeft/aiChat/AiModelSelector.qml:45-96`) добавить
  флаг `isDefault`; подсветка строки «Default» и подпись в subtitle.
- Правый клик/длинное нажатие на модель → «Use as default» (записывает
  provider_id + api_format текущей модели); отображение заблокированной
  дефолтной модели — существующая логика «lock → openProviderSettings»
  (`AiModelSelector.qml:418-425`) уже покрывает.
- Не менять фильтры (`catalogFilters`) — выбор по умолчанию ортогонален им.

### 1.6 Mascot (OpenCode): провайдер и API по умолчанию

`mascot.assistant` в `modules/common/Config.qml:738-740` расширить:

```qml
property string provider: "ollama"   // id провайдера OpenCode
property string apiBase: ""          // пусто = дефолт провайдера
property string apiFormat: "openai"  // openai | anthropic | gemini (для custom apiBase)
```

- `scripts/ai/mascot-assistant.py:174-184` (`configure_ollama`) обобщить до
  `configure_provider(config, provider, api_base, api_format)` — писать блок
  провайдера в `OPENCODE_CONFIG_CONTENT` (`runtime_config`,
  `mascot-assistant.py:78-100`): `provider.<id>.options.baseURL` и `npm`
  (`@ai-sdk/openai-compatible` для openai-формата). Ключи остаются в
  `auth.json` OpenCode (`opencode auth login`) — не дублировать хранилище
  ключей; iNtoo передаёт выбор провайдера/API, но не секреты.
- `services/MascotAssistant.qml:31-37` — собирать `provider/apiBase/apiFormat`
  в JSON-запрос рядом с `model`.
- UI: `modules/settings/MascotConfig.qml:24-29` и
  `modules/waffle/settings/pages/WMascotPage.qml:24-25` — заменить голое поле
  модели на: dropdown провайдера (ollama, anthropic, openai, google, groq,
  mistral, opencode-zen, opencode-go + custom), поле модели, опциональные
  apiBase/apiFormat при custom. При провайдере `ollama` поле модели может
  остаться как есть (совместимость: `ollama/<model>`).
- Обновить `docs/ASSISTANT.md` (секция «The companion defaults to…»).

### 1.7 Исправления assistant (дефекты, найденные при анализе)

| # | Проблема | Место | Исправление |
|---|---|---|---|
| A1 | Шипованные `defaults/config.json` `ai.extraModels` (gemini-2.5-flash, gemini-3-flash-preview, mistral-medium-2505) без `provider_id` дублируют live-каталоги `google-gemini`/`mistral` пресетов (старые endpoint'ы, двойные записи в списке моделей) | `defaults/config.json`, `Config.qml:717-734` | Проставить `provider_id` в шипованные строки (станут guided-fallback, `Ai.qml:818-832` уже умеет их резать при живом каталоге); устаревшие строки убрать из defaults. Для существующих пользователей — автодетект в `_syncExtraModels` по endpoint/key_id → пресет (без перезаписи пользовательского конфига) |
| A2 | `models` объявлен странным тернарником `policy === 2 ? {} : ({})` — обе ветки пустой объект, читается как мусор | `Ai.qml:747` | `property var models: ({})` |
| A3 | `_resolvedCurrentModelId` может вернуть заблокированную модель (`modelList[0]`) → «connect provider» тупик вместо рабочего фолбэка | `Ai.qml:753-764` | Цепочка из 1.2; на каждом шаге только `modelCanRun` |
| A4 | `saveChat("lastSession")` на каждый ответ (`Ai.qml:1296,1537`) — полный `JSON.stringify` транскрипта на каждый токен-финализ; на длинных чатах — пик CPU/RAM и IO (см. M4) | `Ai.qml:1770-1807` | Лимит `lastSession` (последние N сообщений, напр. 200) + дебаунс записи; опция `ai.saveLastSession` (bool, default true) |
| A5 | Транскрипт растёт неограниченно; tool-выводы (`get_shell_config` → весь конфиг) попадают в контекст и в `messageByID` (см. M2) | `Ai.qml:1699-1703,1101-1104` | Обрезка `rawContent` tool-выводов, отправляемых модели (напр. 8 КиБ), кольцевой лимит сообщений (M2) |
| A6 | `loadChat` кладёт числовые id (`0..n-1`), а `idForMessage` генерирует строковые — смешанные типы ключей, хрупко для будущих операций | `Ai.qml:1851-1854` | Нормализовать id при загрузке (строковые), как в `addMessage` |
| A7 | Стриминг: `AiMessage.messageBlocks` пересчитывает `splitMarkdownBlocks(content)` на каждый пришедший токен; `ScriptModel` + Repeater пересоздают делегаты — квадратичный по длине ответа рост GC/CPU (косвенно и RAM) | `modules/sidebarLeft/aiChat/AiMessage.qml:25,252-258` | Троттлинг: пересчёт блоков не чаще ~250 мс во время стрима + финальный пересчёт при `done` (это же пункт в Части 2/3) |
| A8 | `AiProviderCatalog.ensureInitialized` при первом открытии чата запускает до 9 последовательных python-процессов discovery (даже для провайдеров без ключа) | `services/ai/AiProviderCatalog.qml:70-90,112-160` | Пропускать requiresKey-провайдеров без ключа (publicCatalog оставить), stagger по времени, уважать профиль малой памяти (`MemoryPressureService.lowMemory`) — пропуск не-public каталогов. Связано с Частью 2, п. L13 и M1 (diff-обновление) |
| A9 | `setApiKey`/`/key` не экранируют перевод строки и trim'ят пробелы только по краям — ключ с внутренним пробелом сохранится как есть (ок), но multiline сломает `secret-tool` payload | `Ai.qml:1199-1221`, `KeyringStorage` | Валидация: reject ключей с `\n`/`\r`, длина ≤ 4096 |
| A10 | `/test` команда с гигантским markdown-пресетом живёт в рантайм-файле UI | `modules/sidebarLeft/AiChat.qml:169-225` | Вынести за флаг `QS_DEBUG` (как `_log`) либо удалить |
| A11 | `show-installed-ollama-models.sh` не используется нигде | `scripts/ai/` | Удалить или задокументировать назначение |
| A12 | Mascot: на каждое сообщение поднимается новый python+opencode процесс (старт ~сотни мс) | `services/MascotAssistant.qml:58-100` | Опционально: persistent worker с протоколом один JSON-запрос/ответ (сложнее, фаза 2; сразу не делать) |
| A13 | В Waffle-семействе «Connect provider» из селектора моделей и статус-строки чата открывает настройки без `QS_SETTINGS_PAGE` — пользователь попадает не на страницу AI | `GlobalStates.qml:117-136` (ветка isWaffle не передаёт env; `waffleSettings.qml:161` env читает), `AiModelSelector.qml:105-109`, `AiChat.qml:642` | Передавать `QS_SETTINGS_PAGE` и в Waffle-ветке с маппингом ii:24 ↔ waffle:15 (`WSettingsContent.qml:46`); для iris-оверлея индекс 24 уже корректен |

### 1.8 Тесты

- `scripts/test-ai-default-provider.py` (новый, unittest в стиле
  `scripts/test-mascot-assistant.py`): резолвер defaultProvider/defaultModel/
  defaultApiFormat — с моками Config/каталога; приоритеты (persisted > default >
  recommended > runnable > none); политика local-only; id custom-моделей.
- Расширить существующие QML-проверки, если в `scripts/` есть QML-парсеры
  (см. `scripts/test-*`): паритет ключей схемы между `Config.qml` и
  `defaults/config.json` (регрессия A1/A2).
- `scripts/test-mascot-assistant.py`: новые кейсы для
  `configure_provider(provider, apiBase, apiFormat)` и JSON-запроса с полями
  провайдера.
- Ручная проверка: `make test-local`; на живой сессии — смена дефолта в
  настройках, `inir ai diagnose` (уже выводит currentModelId/endpoint/requiresKey,
  `Ai.qml:46-77` — добавить defaultProvider/defaultApiFormat в вывод).

---

## Часть 2. Ускорение загрузки без роста RAM

> Правило: только оптимизации, не увеличивающие RSS (перенос работы позже,
> ленивые инстанцирования, отложенные Process, асинхронные изображения,
> переиспользование уже вычисленного).

### 2.1 Уже сделано (не дублировать)

- Компиляция только активного семейства — `shell.qml:706-794`
  (`FamilyPanelHostLoader` + `Component.Asynchronous`), фикс из
  `PERFORMANCE_ISLAND_AUDIT.ru.md:75-90`.
- Tiered-старт сервисов: Tier 0 критичные / Tier 3 (+500 мс) / Tier 4
  (+1500 мс) — `shell.qml:94-180`.
- Boot-профилирование в `~/.cache/inir/last-boot.json` (`shell.qml:71-80`,
  `inir status`).

### 2.2 Кандидаты (по приоритету, из аудита загрузки)

**L1. Bash-Process на критическом пути конфига (ВЫСОКИЙ).**
`Directories.qml:136-148` `configPathResolver` (`bash -c 'source config-path.sh && inir_config_dir'`, `running: true`): `Config.filePath` ждёт `shellConfigResolved`, т.е. чтение пользовательского конфига начинается только после spawn bash+source (~10–30 мс последовательной задержки до `Config.ready`). Исправление: резолвить каталог в `scripts/inir` (launcher) и передавать через env `INIR_CONFIG_DIR`, в QML — fallback; либо однократное чтение с кэшем в state-файле. RAM: 0.

**L2. 14 форков `execDetached` в Component.onCompleted Directories (ВЫСОКИЙ).**
`Directories.qml:160-172` — 12 вызовов (mkdir/rm -rf/mkdir для stateUserPath, favicons, coverArt, booruPreviews, latexOutput, cliphistDecode, aiChats, screenTimePath, tempImages) + ещё 2 в `onShellConfigResolvedChanged` (`:153-157`). Каждый — fork/exec в разгар boot-contention. Исправление: один `/bin/sh -c "mkdir -p ... && rm -rf ... && mkdir -p ..."`; чистки кэшей (booruPreviews, latexOutput, cliphistDecode, tempImages) перенести в Tier 3 или делать лениво при первом использовании. RAM: 0.

**L3. Внешние темы/иконки выполняются в окне первого кадра (ВЫСОКИЙ).**
`shell.qml:232-233` (+`:120-121`): `ThemeService.applyCurrentTheme()` + `IconThemeService.ensureInitialized()` через Qt.callLater — до shellEntryTimer. Внутри: auto-тема → `MaterialThemeLoader.reapplyTheme` → `requestExternalApply` → bash `scripts/colors/applycolor.sh` (`MaterialThemeLoader.qml:452-480`); manual-тема → `ThemePresets.applyPreset` → execDetached (`ThemePresets.qml:3937,3980`); иконки → `icon-theme-for-mode.sh`, gsettings/kde/qt5ct/qt6ct (`IconThemeService.qml:167-263`). Исправление: разделить «в памяти» (m3colors/токены — обязаны быть до первого кадра) и «внешнее» (все bash/gsettings) — внешнее после `GlobalStates.shellEntryReady`; `IconThemeService.ensureInitialized` целиком за первый кадр. RAM: 0.

**L4. Python-процесс датчиков в первом кадре (бар) (ВЫСОКИЙ).**
`modules/bar/Resources.qml:15` → `ResourceUsageMonitor` → `ResourceUsage.keepAlive()` (`ResourceUsage.qml:188-207`) → `python3 scripts/detect_sensors.py` + findCpuMaxFreqProc + первые поллинги — в момент сборки бара. Исправление: гейт старта мониторинга по `GlobalStates.shellEntryReady` (1–2 кадра «—»), либо кэш путей датчиков на диске между загрузками. RAM: 0.

**L5. `pw-dump` (полный граф PipeWire) на +120 мс (СРЕДНИЙ).**
`MprisController.qml:398-400` → таймер 120 мс → `pw-dump` + JSON.parse всего графа (`:293-334`) внутри окна первого кадра. Исправление: запускать после обнаружения первого MPRIS-плеера или после первого кадра. RAM: 0.

**L6. Tier 0 компилирует SettingsPageRegistry (1974 строки) + Translation (СРЕДНИЙ).**
`shell.qml:108` `DevNavigation.registerSettingsPages(SettingsPageRegistry.pages)` → материализация Translation (сканы каталогов переводов) в первом кадре. Исправление: регистрация страниц в Tier 3; IPC-таргеты уже в `shell.qml:591-645`. RAM: 0.

**L7. GameMode стартует раньше своей Tier-3 (СРЕДНИЙ).**
`Appearance.qml:88-91` и `ToastManager.qml:16` читают `GameMode?.active`; ToastManager инстанцируется без гейта на `shell.qml:940` (до Config.ready!) → GameMode + mkdir/initTimer/startupNiriSync (`GameMode.qml:229-233`). Исправление: заменить чтения на флаги в Appearance/GlobalStates, обновляемые по сигналам GameMode после его реальной загрузки; либо ToastManager в LazyLoader с `active: Config.ready`. RAM: 0.

**L8. YtMusic (2262 строки) компилируется при выключенной фиче (СРЕДНИЙ, +RAM-выигрыш).**
`modules/bar/BarContent.qml:264` `barSpectrumAudioPlaying: MprisController.isPlaying || YtMusic.isPlaying` — YtMusic материализуется всегда при дефолтном `sidebar.ytmusic.enable: false`. Исправление: `|| ((Config.options?.sidebar?.ytmusic?.enable ?? false) && YtMusic.isPlaying)` — short-circuit; синглтон не создаётся и не держится в RAM.

**L9. Burst на T+500 мс + резиденты sidebars (СРЕДНИЙ, +RAM-выигрыш).**
`shell.qml:145-165` одним `deferredPanelsReady` активирует в `ShellIiPanelsImpl.qml` сразу Lock, MediaControls, Polkit, RegionSelector, ScreenCorners, WorkspaceStrip, NotificationPopup, OSD, ripple-Variants и SidebarLeft/SidebarRight (`:296-297`). Исправление: (а) iiSidebarLeft/iiSidebarRight перевести в OnDemandPanelLoader (тяжёлый Ai-UI сейчас резидентен всегда); (б) разнести DeferredPanelLoader по приоритетам (Lock/Polkit/MediaControls — сразу, остальное — позже/по запросу). RAM: (а) — снижение.

**L10. Парсинг неактивных файлов при T+0 через `source:` (НИЗКИЙ).**
LazyLoader::setSource сразу создаёт QQmlComponent — актуально для `shell.qml:678-702` (7 роутеров, в т.ч. JapaneseLookup 487 строк), `shell.qml:683-688` (AltSwitcher.qml — 2080 строк, компилируется не только для ii) и `shell.qml:1071-1079` (FamilyTransitionOverlay.qml — 32 КБ, нужен только при переключении семейства). Исправление: паттерн `Qt.createComponent(..., Component.Asynchronous)` по условию, как в `shell.qml:706-794`. RAM: 0.

**L11. Weather в дефолтном ii-баре (НИЗКИЙ).**
`defaults/config.json:1298-1301` `bar.weather.enable: true` → WeatherBar → Weather (1134 строки) в первом кадре. Варианты: оставить (видимая фича) или LazyLoader `loading`/`activeAsync` для выноса парсинга. Waffle: `WeatherButton.qml:15,19` — аналогично. RAM: 0.

**L12. Мелочи первого кадра — сгруппировать и отложить (НИЗКИЙ).**
`Audio.qml:482-486` (mic-стейт + themeSoundsProc), `Battery.qml:60-62` (chargeLimitDetector), `Network.qml:228-234` (`pkill -f "nmcli monitor"` + subscriber), `Appearance.qml:100-116` (`niri --version` probe; можно кэшировать результат в state-файл). По 1–2 процесса каждый; суммарно заметный фон. RAM: 0.

**L13. AI-специфика (пересечение с Частью 1 и 3).**
- Каталог моделей: при живом кэше `model-catalog.json` отдавать модели сразу, discovery — фоном порциями (A8); diff-обновление вместо пересоздания (M1).
- AiMessage: троттлинг markdown-блоков при стриминге (A7).
- `ThemePresets` (M7): 61 пресет в 153 КБ-файле нужен только как `getPreset(currentTheme)` — ленивая загрузка убирает парсинг из старта (см. `Appearance.qml:750`).

---

## Часть 3. Экономия оперативной памяти

### 3.1 Уже сделано

- `MemoryPressureService` измеряет `MemAvailable`, RSS/PSS и JSGCHeap-маппинги
  (`services/MemoryPressureService.qml:91-154`), есть IPC `inir memory stats`.
  Низкий порог уже рулит: retention панелей (`modules/ii/ShellIiPanelsImpl.qml:65,76`,
  waffle `:76,87`, sidebar `:107,145`), эффекты (`Appearance.qml:95`), видео-обои
  (`modules/iris/components/IrisWallpaperView.qml:31`).
- Многие виджеты освобождают blur/mask framebuffer'ы при скрытии
  (`docs/OPTIMIZATION.md:285`), Cava — один процесс на все мониторы (там же).
- Ограниченные кэши: `WallpaperLuma` (6 карт, очередь ≤3), `MediaArtworkCache`
  (64), `Background._wallpaperSizeCache` (LRU 64, `Background.qml:461-483`),
  Cliphist (400), MascotAssistant (40), YtMusic recent/liked (10/200),
  ResourceUsage-истории (60).

### 3.2 Кандидаты (по приоритету, из аудита памяти)

**M1. Каталог моделей AI как QtObject ×3 (ВЫСОКИЙ, до ~6–12 МБ).**
`Ai.qml:845-903` создаёт `AiModel`-QtObject (~31 свойство) на **каждую** модель
каталога; OpenRouter отдаёт 3000+ моделей. Данные живут трижды:
JS-записи `AiProviderCatalog.models` + `_catalogByProvider` (`AiProviderCatalog.qml:21,35`)
+ AiModel-объекты + JSON-кэш на диске. Дополнительно `_syncCatalogModels`
пересоздаёт **все** AiModel при каждом `catalogUpdated` (массовый мусор
JSGCHeap — тот самый счётчик MemoryPressureService). Исправление: plain
JS-объекты в `Ai.models`, материализация AiModel только для выбранной модели;
diff-обновление вместо пересоздания; опциональный лимит моделей на провайдера
(200 + «показать все»); удалить мёртвый `providerCounts` (`Ai.qml:855,864`).

**M2. Транскрипт Ai-чата без лимита + дублирование текста (ВЫСОКИЙ, утечка).**
`Ai.qml:221-222` `messageIDs/messageByID` растут неограниченно; дубли:
`content`+`rawContent` (×2), tool-вывод в `content` И `functionResponse` (×3);
`get_shell_config` вставляет весь конфиг (~100 КБ) → ~300 КБ строк за один
tool-call; `run_shell_command` копит stdout без лимита (`Ai.qml:1673-1683`).
Исправление: кольцевой лимит N=200 последних сообщений с `destroy()` старых
(UI виртуализирован — `AiChat.qml:441-452` — пользователь не заметит); один
источник текста (`content` из rawContent); лимит `functionResponse` 64 КиБ;
при `MemoryPressureService.lowMemory` — трим до 20 сообщений.

**M3. История нотификаций без лимита + полная ресериализация (ВЫСОКИЙ, утечка).**
`Notifications.qml:134,456` `root.list` растёт навсегда (wrapper-объекты
Notif); `notifFileView.setText(stringifyList(root.list))` с pretty-print —
**на каждое событие** (`:483`, также на discard `:518,546,571`); `_updateGroups`
пересобирает группы при каждом изменении (`:222-243`). Исправление:
конфигурируемый лимит истории (200) с destroy старых; дебаунс персиста 500 мс;
компактный JSON без indent; не персистить `image` у старых записей.

**M4. `saveChat("lastSession")` на каждый ответ (СРЕДНИЙ, транзиентные пики).**
`Ai.qml:1296,1537` → `chatToJson` строит полную копию транскрипта и
`JSON.stringify` на каждый финал ответа; `chatSaveFile` держит последний
сериализованный транскрипт в буфере (`Ai.qml:1792-1797`). Исправление: дебаунс
3–5 с; компактный JSON; `chatSaveFile.path = ""` после записи (как в
`promptLoader`, `Ai.qml:1086-1088`); лимит lastSession (см. A4).

**M5. Конфиг в памяти в 3–4 копиях (СРЕДНИЙ-НИЗКИЙ).**
`Config.qml:398` `_jsonMirror` + дерево JsonAdapter + буфер `configFileView.text()`
+ `rawConfigReader` (полный текст для `_reloadIfExternal`, `:550-560`).
Каждая запись — pretty-print `JSON.stringify(obj, null, 4)` (`:499,528`).
Исправление: компактная сериализация; сравнение внешних изменений по
hash/размеру вместо второго FileView; ленивый snapshot-зеркало.

**M6. Глобальный QPixmapCache от `cache: true` в галереях (СРЕДНИЙ-НИЗКИЙ).**
Галереи/миниатюры с уникальными URL вытесняют постоянные иконки панелей и
держат RSS у лимита кэша Qt (~10 МБ): `WallpaperCoverflowGallery.qml:896,916,1175,1327`,
`WallpaperSkewView.qml:1029,1049,1059,1283`, `YtMusicTrackItem.qml:112`,
`AnimeCard.qml:102`, `SidebarLeftContent.qml:374`. Исправление: `cache: false`
для преходящих миниатюр, `cache: true` только для постоянных ресурсов.
Исключение: `WallpaperCrossfader.qml:443-449` уже освобождает кэш — не трогать.

**M7. Статика, разбираемая при старте (НИЗКИЙ-СРЕДНИЙ).**
- `ThemePresets.qml:30` — 61 пресет в файле 153 КБ, нужен только
  `getPreset(currentTheme)` (`Appearance.qml:750`) → вынести в JSON/ленивую
  загрузку (пересекается с Частью 2, п. L13).
- `GlobalActions.qml:1039-1049` `searchActions` — полный дубль реестра
  (~300 объектов) ради одного потребителя `LauncherSearch.qml:56` → строить в
  потребителе на лету.
- `Ai.qml:295-689` `tools` — статический JS ~60 КБ схем для 6 форматов;
  нужен один формат на запрос → ленивый JSON/файл, материализация по
  `api_format`.
- `Ai.qml:771-777` `apiStrategies` — 5 стратегий создаются биндингом сразу →
  ленивый геттер.

**M8. Мелкие неограниченные map-кэши (НИЗКИЙ).**
`Wallpapers.qml:466,496` `_knownThumbnailOutputs`/`_failedThumbnailOutputs` —
LRU 1000; `WallpaperLuma.qml:22` `_failed` — лимит 64;
`NewsService.qml:28` `_cache` — LRU 10; `Booru.qml:25` `responses` — очистка
при закрытии просмотра (низкий приоритет).

**M9. Давление памяти не покрывает сервисные данные.**
`lowMemory` не влияет на Ai-транскрипт (M2), историю нотификаций (M3),
каталог моделей (M1), кэши (M8). Исправление: подписать
Ai/Notifications/Wallpapers на `MemoryPressureService.lowMemory` (триммить/
сокращать лимиты) и `forceGc()` (`MemoryPressureService.qml:36-39`) при входе
в режим; снизить интервал проверки (сейчас 5 мин, `:20`) при приближении к
порогу.

### 3.3 Проверено и признано НЕ проблемой

Todo, WorldClock, JapaneseDictionary, LyricsService, Cliphist, IconThemeService,
Autostart, DesktopItems, MascotChaos/Mood, CavaService (subscriber-gated),
WindowPreviewService, Backdrop/Background (sourceSize ограничивает decode,
`Background.qml:1542`; у обоев `cache:false`), FileView watchChanges.

---

## Порядок работ, коммиты и PR

Ветка от `prerelease`: `feat/assistant-default-provider` (часть 1),
`perf/shell-startup` (часть 2), `perf/shell-memory` (часть 3) — правила
`CONTRIBUTING.md:58-64`. Один логический коммит на изменение, subject
`type(scope): описание` < 60 символов без точки.

| Фаза | Коммиты (предлагаемые) | Содержание |
|---|---|---|
| 1 | `feat(ai): add default provider and api format config` | 1.1 + 1.2 (схема, резолвер, defaults, Persistent) |
| 1 | `feat(ai): add deepseek provider preset` | 1.3 |
| 1 | `feat(ai): default provider controls in settings and chat` | 1.4 + 1.5 (ii + Waffle паритет) + A13 (QS_SETTINGS_PAGE для Waffle) |
| 1 | `feat(mascot): provider and api selection for opencode` | 1.6 + python + тесты |
| 1 | `fix(ai): resolve default model, bound transcript, dedupe legacy rows` | A1–A6, A8–A12 + M1 (каталог plain-JS + diff), M2 (лимит транскрипта), M4 (lastSession) |
| 2 | `perf(ai): throttle streamed markdown and stagger catalog discovery` | A7 (троттлинг блоков при стриминге) + A8 (кэш каталога сразу, discovery порциями) + L13 |
| 2 | `perf(boot): resolve config dir without subprocess` | L1 + L2 (env INIR_CONFIG_DIR, слияние 14 форков) |
| 2 | `perf(boot): defer external theming and sensors past first frame` | L3 + L4 + L5 + L12 |
| 2 | `perf(boot): lazy ytmusic, settings registry and game mode` | L6 + L7 + L8 |
| 2 | `perf(boot): compile inactive routers on demand` | L10 (AltSwitcher, FamilyTransitionOverlay, роутеры) |
| 2 | `perf(panels): on-demand sidebars and staggered deferred burst` | L9(а,б) + L11 |
| 3 | `perf(notifications): cap history and debounce persistence` | M3 |
| 3 | `perf(ai): cache pixmaps, compact config copies, lazy statics` | M5 + M6 + M7 + M8 |
| 3 | `perf(memory): wire lowMemory into ai, notifications, catalogs` | M9 |
| 4 | `docs(ai): default provider and assistant fixes` | docs/ASSISTANT.md, docs/FLASH_DEEPSEEK_V41_PLAN.ru.md |

Каждый PR: описание проблемы → поведение → команды и наблюдения (обязательно
живой запуск, `CONTRIBUTING.md:213-230`), скриншоты для UI-изменений, чеклист
AI-атрибуции.

## Метрики и проверка результата

- **Загрузка**: `~/.cache/inir/last-boot.json` (`shell.qml:71-80`) до/после —
  дельты T+0 (Component.onCompleted), Config.ready, shellEntryReady, deferred,
  late. Цель: без регрессии T+0 и с уменьшением «Launching config → первый
  кадр» на холодном старте; сравнение минимум по 3 перезапускам.
- **RAM**: `inir memory stats` (MemoryPressureService) и `/proc/<pid>/smaps_rollup`
  (PSS) после 30 минут idle, после открытия/закрытия AI-чата и после длинного
  диалога. Цель: нет роста RSS на idle, снижение пиков GC (JSGCHeap mappings
  deleted counter).
- **Assistant**: `inir ai diagnose` — defaultProvider/defaultApiFormat/
  currentModelId/endpoint корректны; ручной сценарий: выбор DeepSeek по
  умолчанию → рестарт → первый запрос уходит на DeepSeek с OpenAI-форматом;
  политика Local-only не выбирает online-провайдер.
- Регрессии: `make test-local`, `python3 scripts/test-mascot-assistant.py`,
  новый `scripts/test-ai-default-provider.py`, `node scripts/test-idle-policy.js`
  (если затронуты таймеры).

## Риски и открытые вопросы

1. `defaultApiFormat` может разойтись с фактическим форматом модели каталога
   (Gemini-модели OpenRouter и т.п.) — хранить его только как «намерение для
   custom/пресета», а при выборе конкретной модели всегда побеждает
   `api_format` самой модели (`_syncCatalogModels`, `Ai.qml:880`).
2. Mascot: OpenCode 1.x зачитывает ключи из своего `auth.json`; iNtoo не должен
   хранить/передавать ключи для OpenCode — только provider/apiBase/apiFormat.
   Проверить поддержку `baseURL` в `OPENCODE_CONFIG_CONTENT` для OpenCode 1.18.x
   (зависимость уже закреплена в `scripts/test-mascot-assistant.py`).
3. Изменения `Persistent.qml` (дефолт `ai.model`) требуют проверки миграции
   состояний: существующий `states.json` с `gemini-2.5-flash` останется
   валидным (резолвер просто проигнорирует отсутствующую модель).
4. UI-паритет ii/Waffle обязателен (`CONTRIBUTING.md:93-100`) — любые новые
   строки — через `Translation.tr()` с добавлением в `translations/`.
5. L9(а) — перевод sidebars на OnDemandPanelLoader меняет контракт
   «резидентности» из `docs/OPTIMIZATION.md:275-283` (sidebars — deliberate
   exception): нужен замер времени reopen и решение «on-demand только в
   профиле малой памяти» vs «всегда»; сохранить навигацию/поиск через
   Persistent (уже реализовано).
6. L1 — env `INIR_CONFIG_DIR` должен пройти все пути запуска (systemd-unit,
   OpenRC-супервайзер, `inir run`, repo-link, package-managed), иначе QML
   fallback обязан оставаться корректным (`Directories.qml:136-148`).
7. M1 (plain-JS вместо AiModel-QtObject) затрагивает потребителей
   `Ai.models[...]` в UI (`AiMessage.qml:99-103,132-133`, `AiModelSelector.qml`,
   `AiConfig.qml`) — рефакторинг с контрактным слоем доступа
   (`Ai.getModel()`, `Ai.modelCanRun`), а не прямой заменой типа.
8. M3 — лимит истории нотификаций персистится: обрезка на диске должна быть
   идемпотентной и не удалять непрочитанные «critical» без предупреждения
   (настраиваемый порог, дефолт 200).
