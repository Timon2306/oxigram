<div align="center">

<img src="native/icons/icon.png" width="120" alt="Логотип Oxigram">

# Oxigram

**Нативный клиент Telegram для компьютера на Rust, которому хватает нескольких десятков МБ памяти.**<br>
Rust сам говорит с серверами Telegram по MTProto и сам рисует окно. Без TDLib, Electron, WebView и Qt.

[![Лицензия: MIT](https://img.shields.io/badge/license-MIT-2a8cf0?style=flat-square)](LICENSE) [![Rust](https://img.shields.io/badge/Rust-grammers-dea584?style=flat-square&logo=rust&logoColor=white)](https://codeberg.org/Lonami/grammers) [![Slint](https://img.shields.io/badge/UI-Slint-2379f4?style=flat-square)](https://slint.dev) [![Платформы](https://img.shields.io/badge/platforms-Windows%20·%20Linux%20·%20macOS-0078d4?style=flat-square)](#установка)

[English](README.md) · **Русский**

</div>

## Зачем

Официальные клиенты держат в памяти много: большой кэш медиа, целый набор
интерфейсных библиотек, иногда встроенный браузер. Oxigram держит только то,
что на экране. Сеть, база и окно — одна небольшая программа на Rust: интерфейс
рисует программный рендерер [Slint](https://slint.dev), поэтому в памяти нет ни
браузерного движка, ни драйвера видеокарты.

- **Около 10–30 МБ ОЗУ** с открытым окном (больше, пока на весь экран открыто
  фото или видео, и снова меньше, когда закрыто). Точная цифра всегда в
  заголовке окна.
- **Исполняемый файл ~15 МБ**, установщики около 7 МБ.
- Картинки декодируются сразу в том размере, в каком нарисованы, и
  выбрасываются, когда вы уходите из чата; в длинных чатах в памяти только
  строки на экране.

## Возможности

<table>
<tr>
<td width="50%" valign="top">

**💬 Чаты**
- Папки вкладками, архив, закреплённые чаты
- Поиск по всем чатам и внутри одного (Ctrl+F)
- Без звука, прочитано/непрочитано, архив, очистка, выход
- Черновики по чатам, контакты, колонка профиля
- «Печатает…» в шапке и в списке чатов

</td>
<td width="50%" valign="top">

**✉️ Сообщения**
- Ответ, правка, пересылка, удаление, закрепление
- Выбор нескольких сообщений, чтобы переслать или удалить
- Реакции (и кастомные эмодзи), галочки прочтения
- Цветные эмодзи без видеокарты
- Быстрое форматирование: `**жирный**`, `__курсив__`, `||спойлер||`

</td>
</tr>
<tr>
<td valign="top">

**🖼️ Медиа**
- Фото и альбомы, просмотр на всё окно
- Видео играет внутри программы (Windows 8+, macOS, Linux с GStreamer)
- Голосовые с волной, музыка
- Отправка файлов кнопкой, Ctrl+V или перетаскиванием; альбомы, прогресс, отмена
- «Экономия памяти»: переключатели для аватарок, картинок, GIF, кастомных эмодзи

</td>
<td valign="top">

**🖥️ Компьютер**
- Уведомления с учётом заглушённых чатов; «выключить все» в трее
- Работа из трея, автозапуск с системой, только одна копия
- Код-пароль: данные на диске зашифрованы (Argon2id, ChaCha20-Poly1305), автоблокировка, Ctrl+L
- Прокси SOCKS5 и HTTP или встроенный обход блокировок через WebSocket
  (веб-адреса Telegram, свой сервер не нужен), ещё до ввода ключей API
- Светлая и тёмная темы, размер текста
- Клавиатура: Ctrl+Tab, ↑ для правки, Ctrl+F, Esc

</td>
</tr>
</table>

**Пока нет:** MTProto-прокси, анимированные (Lottie) стикеры, звонки,
истории, несколько аккаунтов, запись голосовых, секретные чаты.

## Установка

Установщики лежат в [релизах](https://github.com/Timon2306/oxigram/releases):

| Файл | Система |
|---|---|
| `oxigram-windows-x64-…-setup.exe` | Windows 10/11, 64 бита |
| `oxigram-windows-x86-…-setup.exe` | Windows 10/11, 32 бита |
| `oxigram-windows-arm64-…-setup.exe` | Windows 11 на ARM |
| `oxigram-linux-x64-….deb` | Ubuntu 20.04+, Debian 11+, Mint |
| `oxigram-linux-x64-….AppImage` | другие дистрибутивы (glibc 2.31+: ALT p10, Fedora 33 и новее) |
| `oxigram-macos-apple-silicon-….dmg` | Mac на M1/M2/M3/M4 (macOS 11+) |
| `oxigram-macos-intel-….dmg` | Mac на Intel (macOS 10.13 High Sierra+) |

Сборки без цифровой подписи. В Windows, если SmartScreen предупреждает:
**Подробнее → Выполнить в любом случае**. В macOS при первом запуске: правый
клик по Oxigram → **Открыть**.

## Как начать

Oxigram поставляется **без** `api_id` / `api_hash` — нужны свои.

1. Войдите на <https://my.telegram.org> → **API development tools**.
2. Создайте приложение (название любое), скопируйте `api_id` и `api_hash`.
3. Запустите Oxigram, вставьте их и войдите по номеру телефона.

Если Telegram у вас заблокирован, сначала нажмите **Настроить прокси** на
первом экране (ключи проверяются только подключением) и включите **Обход
блокировок (WebSocket)**: свой сервер для него не нужен. Или впишите прокси
SOCKS5 либо HTTP.

Ключи и сессия хранятся только на вашем компьютере и не уходят никуда, кроме
серверов Telegram.

## Сборка из исходников

Нужен [Rust](https://rustup.rs) (stable).

```sh
git clone https://github.com/Timon2306/oxigram
cd oxigram
cargo run --release -p oxigram-native
```

В Linux сначала поставьте пакеты разработки (названия для Debian/Ubuntu):

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libfontconfig1-dev libgtk-3-dev libayatana-appindicator3-dev \
  libdbus-1-dev libasound2-dev libssl-dev
# видео внутри программы: ещё libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev,
# и сборка с --features gstreamer
```

Установщики собираются через [cargo-packager](https://github.com/crabnebula-dev/cargo-packager)
(`cargo install cargo-packager --locked`, затем `cargo packager --release` в
папке `native/`). Workflow `.github/workflows/release.yml` собирает их все;
он запускается только вручную.

<details>
<summary><b>Тесты и устройство проекта</b></summary>

<br>

```sh
cargo test -p oxigram-core -p oxigram-native   # юнит-тесты
cargo test -- --ignored                        # ещё и с обращением к серверам Telegram
```

```
core/src/            бэкенд (без интерфейса)
  tg.rs              подключение, вход, прокси
  chats.rs           диалоги, история, профили, папки, загрузка медиа
  actions.rs         закрепить, архив, реакции, правка, пересылка, удаление,
                     отправка файлов, поиск, контакты
  updates.rs         живые обновления, уведомления, поток событий
  proxy.rs           настройки прокси и проверка соединения
  proxy_http.rs      HTTP CONNECT через локальный мост SOCKS5
  proxy_ws.rs        обход через WebSocket (wss-адреса Telegram Web)
  markup.rs          **жирный** / __курсив__ → разметка сообщения
  media.rs           что за вложение и какую миниатюру скачать
  store/             локальная SQLite: сессия и кэш чатов
  mem.rs             замер памяти процесса (заголовок окна)
native/              программа: окно на Slint и его часть на Rust
  ui/app.slint       весь интерфейс
  src/main.rs        запуск, список чатов, история
  src/pictures.rs    картинки, альбомы, просмотр
  src/video/         видео для каждой системы (Media Foundation,
                     AVFoundation, GStreamer)
  src/audio.rs       голосовые и музыка
  src/sending.rs     отправка файлов
  src/…              реакции, поиск, папки, трей, уведомления, прокси…
  vendor/            программный рендерер Slint с цветными эмодзи
```

До версии 0.1 окно Oxigram рисовалось через WebView (Tauri + Svelte); с 0.2 программа полностью нативная.

</details>

## Переводы

Интерфейс на английском, русском и украинском: язык берётся из системы или
из **Настройки → Общие → Язык** и переключается сразу.

Как добавить язык:

1. Скопируйте `native/translations/ru/LC_MESSAGES/oxigram-native.po` в
   `native/translations/<код>/LC_MESSAGES/oxigram-native.po` (`<код>` —
   код языка: `de`, `pl`, `kk`…).
2. Укажите `Language:` и `Plural-Forms:` в заголовке (см.
   [список gettext](https://www.gnu.org/software/gettext/manual/html_node/Plural-forms.html))
   и переведите каждый `msgstr`, сохраняя `{}` и `{n}`. Файл открывается в
   [Poedit](https://poedit.net) как есть.
3. Добавьте язык в `LANGUAGES` и `resolve` в `native/src/i18n.rs`, а правило
   множественного числа в `plural_index`.
4. `cargo test -p oxigram-native i18n` проверит, что переведено всё, затем
   откройте pull request.

## Честная игра

Oxigram — неофициальный клиент на Telegram API, не связанный с Telegram. Он
соблюдает [условия использования API](https://core.telegram.org/api/terms):
никакого «режима призрака», скрытого прочтения или скрытого набора текста,
никакого сохранения удалённых и самоуничтожающихся сообщений.

## Лицензия

[MIT](LICENSE) © Timon2306

Используйте, меняйте и распространяйте Oxigram свободно, в том числе в своих
проектах. Одно условие: сохраняйте уведомление об авторских правах из
[LICENSE](LICENSE) с указанием Timon2306 в каждой копии и в каждом проекте на
основе этого кода.

Интерфейс сделан на Slint по его бесплатной лицензии Royalty-free (программный
рендерер и ядро Slint в `native/vendor/` под теми же лицензиями; см. файлы там).

Цветные флаги стран взяты из шрифта «Twemoji Country Flags»
([country-flag-emoji-polyfill](https://github.com/talkjs/country-flag-emoji-polyfill),
MIT), рисунки [Twemoji](https://github.com/twitter/twemoji) по лицензии
[CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/); см.
`native/fonts/OxigramFlags-LICENSE.md`. Шрифт Open Sans под SIL OFL.

<a href="https://slint.dev"><img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" height="48" alt="Made with Slint"></a>
