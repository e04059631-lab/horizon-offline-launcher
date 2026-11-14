# ⛏️ Horizon Offline Launcher

<p align="center">
  <img src="assets/icon.png" alt="Horizon Launcher Logo" width="128" height="128">
</p>

<p align="center">
  <strong>Автономный оффлайн-форк Horizon Launcher на Rust с интерфейсом Material 3 Dark под версию 26.3.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Fork%20Of-Horizon%20Launcher-FF8A65?style=for-the-badge" alt="Fork of Horizon">
  <img src="https://img.shields.io/badge/Language-Rust-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/License-No%20License%20Required-00b894?style=for-the-badge" alt="No License">
  <img src="https://img.shields.io/badge/Assets-Pre--installed%20%28Zero%20Download%29-F57C00?style=for-the-badge" alt="Pre-installed Assets">
  <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-4E73DF?style=for-the-badge" alt="Platforms">
  <img src="https://img.shields.io/badge/UI-Material%203%20egui-7D5260?style=for-the-badge" alt="Material 3">
</p>

---

## 📌 О проекте (Форк Horizon Launcher)

**Horizon Offline Launcher** это независимый форк лаунчера **Horizon Launcher**, сфокусированный на максимальной автономности, простоте и отказе от внешних облачных зависимостей.

### Ключевые отличия этого форка:

* 🔓 **Работает без лицензии (No License / Offline):**
  Полная свобода от учетных записей Microsoft, онлайн-авторизаций и проверки владения игрой. Достаточно просто ввести желаемый никнейм и запустить игру в один клик.

* 📦 **Все ассеты уже встроены (Zero-Download):**
  Никаких томительных загрузок гигабайтов ресурсов и библиотек при первом старте. Все текстуры, звуки, клиентские архивы и библиотеки версии 26.3 уже аккуратно упакованы внутри исполняемого файла. Запустил и сразу играешь, даже если на компьютере вообще отключен интернет!

* 🚀 **100% чистый Rust:**
  Лаунчер написан на языке Rust с использованием графического фреймворка `egui` / `eframe`. Он мгновенно открывается, потребляет меньше 40 МБ оперативной памяти и не создает лишней нагрузки на систему.

* 🎨 **Дизайн Material 3 Dark (Golden Hour):**
  Современный адаптивный интерфейс с мягкими акцентами, скругленными карточками настроек и встроенными шрифтами Google Sans.

* ⚙️ **Полный контроль над JVM и памятью:**
  Удобная настройка выделения оперативной памяти (от 2 до 16 ГБ), автопоиск Java в системе и гибкая передача аргументов запуска.

* 🐧 **Кроссплатформенность:**
  Одинаково стабильно работает на Windows 10/11 и всех современных дистрибутивах Linux (Ubuntu, Linux Mint, Debian, Arch, Void, Artix).

---

## 🎮 Готовые сборки (Releases)

Готовые автономные сборки со всеми вшитыми ассетами опубликованы на странице **[Релиз v1.0.0](https://gitlab.com/wwerz1/horizon-offline/-/releases/v1.0.0)**:

| ОС | Файл для скачивания | Размер | Особенности |
| :--- | :--- | :--- | :--- |
| **Windows 10 / 11 (x64)** | 🪟 [horizon-launcher-windows-x64.exe](https://gitlab.com/api/v4/projects/88944111/packages/generic/horizon-launcher/v1.0.0/horizon-launcher-windows-x64.exe) | ~423 МБ | Портативный .exe, не требует установки. |
| **Linux (x86_64)** | 🐧 [horizon-launcher-linux-x86_64](https://gitlab.com/api/v4/projects/88944111/packages/generic/horizon-launcher/v1.0.0/horizon-launcher-linux-x86_64) | ~428 МБ | Автономный бинарник для любого Linux-дистрибутива. |

---

## 🛠 Сборка из исходников

Для самостоятельной компиляции лаунчера нужен установленный инструментарий Rust (`cargo` / `rustc`):

```bash
# Клонировать репозиторий
git clone git@gitlab.com:wwerz1/horizon-offline.git
cd horizon-offline

# Собрать релиз
cargo build --release
```

Для сборки под Windows с иконкой и метаданными версии:
```powershell
.\build_release.ps1
```

---

## 📂 Структура репозитория

```text
horizon-offline/
├── src/
│   ├── main.rs              # Главное окно и точка входа eframe
│   ├── config.rs            # Конфигурация и сохранение настроек
│   ├── game_process.rs      # Управление процессом игры и JVM аргументами
│   ├── client_unpacker.rs   # Модуль распаковки и проверки встроенных ресурсов
│   └── ui_bridge.rs         # Мост графического интерфейса
├── assets/                  # Иконки всех размеров (.ico, .png, .svg)
├── .agents/skills/          # Спецификации стилизации Material 3
├── build.rs                 # Скрипт компоновки ресурсов Windows
├── build_release.ps1        # Релизная компиляция
├── Cargo.toml               # Зависимости проекта
├── LICENSE                  # Лицензия MIT
└── README.md
```

---

## ⚖️ Лицензия

Исходный код распространяется под открытой лицензией [MIT](LICENSE).
Minecraft является зарегистрированной торговой маркой Mojang Studios / Microsoft. Данный форк является независимым фанатским проектом.
