# ⛏️ Horizon Offline Launcher

<p align="center">
  <img src="assets/icon.png" alt="Horizon Launcher Logo" width="128" height="128">
</p>

<p align="center">
  <strong>Современный, легкий и быстрый оффлайн-лаунчер для Minecraft (Java Edition 26.3), созданный на чистом Rust.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F?style=for-the-badge&logo=minecraft&logoColor=white" alt="Minecraft 26.3">
  <img src="https://img.shields.io/badge/Language-Rust-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-4E73DF?style=for-the-badge" alt="Platforms">
  <img src="https://img.shields.io/badge/UI-Material%203%20egui-7D5260?style=for-the-badge" alt="Material 3">
  <img src="https://img.shields.io/badge/Mode-Offline%20%2F%20No%20MS%20Login-00b894?style=for-the-badge" alt="Offline">
  <img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="MIT License">
</p>

---

## ✨ Ключевые возможности

* 🚀 **100% Rust & чистый код:** написан на Rust с использованием фреймворка `egui` / `eframe`. Быстрый запуск, минимальное потребление системных ресурсов (RAM < 40 МБ в фоне).
* 🔓 **Полный оффлайн-режим (No License required):** вход по любому никнейму. Никаких обязательных привязок к аккаунтам Microsoft, токенов авторизации и сетевых задержек.
* 📦 **Вшитые ассеты «все в одном»:** готовая версия со всеми распакованными библиотеками, звуками и ассетами клиента 26.3. Не требует долгой докачки гигабайтов при первом старте.
* 🎨 **Интерфейс в стиле Material 3 Dark (Golden Hour):** аккуратные скругления, адаптивная верстка, понятная навигация и системные шрифты Google Sans / Material Symbols.
* ⚙️ **Управление памятью и параметрами JVM:** удобный выбор выделения ОЗУ (от 2 до 16 ГБ), автоматический поиск установленных Java JRE / JDK, гибкие флаги сборщика мусора G1GC.
* 🐧 **Кроссплатформенность:** единая кодовая база для Windows (x64) и Linux (x86_64, проверено на Ubuntu, Linux Mint, Void, Artix).

---

## 🎮 Скачать готовые версии (Релизы)

Готовые сборки со встроенными ассетами доступны на вкладке **Releases**:

| Платформа | Формат | Описание |
| :--- | :--- | :--- |
| **Windows 10 / 11 (x64)** | `.exe` (~423 МБ) | Портативный исполняемый файл. Скачал, запустил и сразу в игре. |
| **Linux (x86_64)** | бинарник (~428 МБ) | Автономный бинарный файл под любые современные дистрибутивы Linux. |

---

## 🛠 Сборка из исходников

Для компиляции лаунчера требуется компилятор Rust (`cargo` / `rustc` 1.80+).

```bash
# Сборка релизной версии
cargo build --release

# Исполняемый файл появится в папке target/release/
```

Для сборки под Windows с иконкой и ресурсами:
```powershell
.\build_release.ps1
```

---

## 📂 Структура репозитория

```text
horizon-offline-launcher/
├── src/
│   ├── main.rs              # Точка входа в приложение и окно eframe
│   ├── config.rs            # Конфигурация лаунчера и сохранение настроек
│   ├── game_process.rs      # Управление процессом игры и аргументами JVM
│   ├── client_unpacker.rs   # Модуль распаковки и проверки целостности клиента
│   └── ui_bridge.rs         # Мост графического интерфейса
├── assets/                  # Иконки всех разрешений (.ico, .png, .svg)
├── .agents/skills/          # Спецификации и документация стилизации Material 3
├── build.rs                 # Скрипт сборки ресурсов Windows
├── build_release.ps1        # Автоматизация релизной сборки
├── Cargo.toml               # Зависимости проекта
└── README.md
```

---

## ⚖️ Правовая информация

Horizon Offline Launcher: независимый проект с открытым исходным кодом. Minecraft является зарегистрированной торговой маркой Mojang Studios и Microsoft Corporation. Данный проект не связан с Mojang Studios или Microsoft.

Лицензия: [MIT](LICENSE).
