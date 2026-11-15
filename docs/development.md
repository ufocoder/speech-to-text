# Разработка S2T

[Вернуться к README](../README.md)

## Сборка и запуск

Проект написан на Rust и использует GPUI Kit. Установите стабильный Rust через rustup. Для macOS понадобятся Xcode и его инструменты командной строки. Для Windows x64 понадобятся инструменты Visual Studio с C++ и Windows SDK. При первой сборке Cargo загружает зависимости, включая нативные библиотеки sherpa-onnx и ONNX Runtime.

Из корня репозитория выполните:

```sh
cargo run --locked
cargo check --locked
cargo test --locked
```

## Автосборка

На macOS установите Watchexec через Homebrew и запустите наблюдение из корня проекта.

```sh
brew install watchexec

watchexec --restart \
  --watch src \
  --watch locales \
  --watch Cargo.toml \
  --watch Cargo.lock \
  --watch build.rs \
  --watch Info.plist \
  -- cargo run --locked
```

Watchexec перезапустит приложение после изменения исходников. Активная запись при перезапуске прерывается. Для проверки без запуска замените команду после `--` на `cargo check --locked`. Остановите наблюдение через Ctrl+C.

## Структура проекта

| Путь | Назначение |
| --- | --- |
| [src/ui](../src/ui) | Экраны и компоненты настольного интерфейса |
| [src/audio](../src/audio) | Запись, подготовка и воспроизведение аудио |
| [src/recognition.rs](../src/recognition.rs), [src/diarization.rs](../src/diarization.rs) | Распознавание речи и определение говорящих |
| [src/models.rs](../src/models.rs) | Каталог моделей, загрузка и проверка файлов |
| [src/storage.rs](../src/storage.rs) | Хранение записей и результатов |
| [locales](../locales) | Русский и английский переводы интерфейса |
| [macos/Resources](../macos/Resources) | Иконка Dock и системные переводы запроса доступа к микрофону |

## Модели

Модели выполняются из Rust. T-one использует жадное CTC-декодирование с состоянием между отрезками. GigaAM и Parakeet обрабатывают длинные записи отрезками до 24 и 60 секунд соответственно.

`SPEECH_MODEL_CACHE` переопределяет каталог кэша. При запуске из исходников на macOS стандартный путь — `.model-cache/hub` в корне репозитория. `SPEECH_MODEL_BASE_URL` задаёт зеркало для файлов по схеме `/<repo>/resolve/<revision>/<file>`. Источники и закреплённые ревизии описаны в [src/models.rs](../src/models.rs).

Исходные репозитории моделей: [T-one](https://huggingface.co/t-tech/T-one), [GigaAM-v3 INT8](https://huggingface.co/nocmt/GigaAM-v3-E2E-CTC-int8-ONNX) и [Parakeet v3 INT8](https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8).

## Локальный сайт

Лендинг находится в [docs](.). Отдельная сборка ему не нужна. Из корня проекта запустите сервер:

```sh
python3 -m http.server 8000 --bind 127.0.0.1 --directory docs
```

Откройте [127.0.0.1:8000](http://127.0.0.1:8000). Кнопки скачивания получают данные о сборках из последнего релиза GitHub. Проверить синтаксис скрипта можно через `node --check docs/app.mjs`. GitHub Pages обновляется при отправке изменений в `docs/` в ветку `main` или при ручном запуске [workflow сайта](../.github/workflows/pages.yml).

## Пакеты macOS

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/build-macos.sh
```

Пакеты появляются в `dist/macos/arm64/` и `dist/macos/x86_64/`. Скрипт включает локализованные ресурсы и подписывает пакеты ad hoc. Для распространения с доверенной подписью macOS используются Developer ID и нотарификация.

Чтобы собирать оба пакета перед отправкой изменений с macOS, включите хук для этого клона:

```sh
git config --local core.hooksPath .githooks
```

При ошибке сборки хук прерывает отправку. Чтобы обновить иконку Dock после правок [SVG](../macos/Assets/Microphone.svg), установите `librsvg` и выполните `./scripts/build-macos-icon.sh` перед упаковкой.

## Релизы

[Workflow настольного релиза](../.github/workflows/macos-release.yml) запускается при отправке тега `v*`. Он собирает ZIP для Apple Silicon, Intel и Windows x64 и прикрепляет их к GitHub Release.

Обновите версию в `Cargo.toml` и `Cargo.lock`, проверьте сборку и закоммитьте изменения. Поставьте новый тег на коммит выпуска. Например, для версии `0.4.0`:

```sh
git tag -a v0.4.0 -m "S2T v0.4.0"
git push origin main
git push origin v0.4.0
```
