# ymtui

Яндекс Музыка в терминале: поиск, любимые треки, плейлисты, «Моя волна» и очередь.
Написан на Rust, интерфейс вдохновлён [spotatui](https://github.com/LargeModGames/spotatui).

## Установка

Windows x64, PowerShell:

```powershell
irm https://github.com/size-prog/Ymtui/releases/latest/download/install-online.ps1 | iex
```

Установщик скачает готовый плеер с libmpv и добавит `ymtui` в PATH.
Rust и права администратора не нужны. Для обновления закройте плеер и повторите команду.
Команда установки требует опубликованного релиза.

Можно скачать ZIP из [Releases](https://github.com/size-prog/Ymtui/releases),
распаковать и запустить `install.cmd`. После этого откройте новый терминал.

## Запуск

```powershell
ymtui --login
ymtui
```

При входе откройте ссылку и подтвердите код в браузере.
`ymtui --demo` запускает деморежим без аккаунта и звука.
Для музыки нужна подписка Яндекс Плюс. Клиент использует неофициальный API.

## Клавиши

| Клавиша | Действие |
| --- | --- |
| `1`–`6` | Разделы библиотеки |
| `Tab` | Сменить панель |
| `↑` / `↓` | Выбрать строку |
| `Enter` | Открыть или включить |
| `/`, `Ctrl+F` | Поиск; `/` в радио — фильтр |
| `Пробел` | Пауза |
| `n` / `p` | Следующий / предыдущий трек |
| `←` / `→` | Перемотка на 5 секунд |
| `+` / `-` | Громкость |
| `l`, `e` | В любимое / в очередь |
| `[` / `]` | Категории радио |
| `Esc` | Назад |
| `?`, `F1` | Остальные клавиши |
| `q` | Выход |

Работают мышь и русская раскладка: например, `х` / `ъ` вместо `[` / `]`.

## Сборка

Нужен Rust, на Windows — также MSVC Build Tools и Windows SDK.

```powershell
cargo build --locked --release
.\target\release\ymtui.exe --demo
```

Для звука при сборке из исходников нужна libmpv. Путь к DLL или её папке
можно указать в `YMTUI_MPV_PATH`. Проверка: `ymtui --check-audio`.

## Удаление

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\Programs\ymtui\bin\uninstall.ps1"
```

Токен и настройки сохранятся. Лицензия проекта — [MIT](LICENSE).
Форматы API сверялись с [yandex-music-api](https://github.com/MarshalX/yandex-music-api).
