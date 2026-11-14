# ⛏ Horizon Offline Launcher

<p align="center">
  <img src="https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F?style=for-the-badge&logo=minecraft&logoColor=white" alt="Minecraft 26.3">
  <img src="https://img.shields.io/badge/Language-Rust-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Renderer-Vulkan-red?style=for-the-badge&logo=vulkan&logoColor=white" alt="Vulkan">
  <img src="https://img.shields.io/badge/Account-Offline%20%2F%20No%20License-00b894?style=for-the-badge" alt="Offline">
  <img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="MIT License">
</p>

Быстрый, современный и простой лаунчер для **Minecraft: Java Edition 26.3**, написанный на чистом **Rust** на базе открытого движка **Horizon**.

Лаунчер запускает игру **без обязательной привязки к Microsoft / лицензии** (полный оффлайн-режим) и поставляется с **готовой предустановленной версией** (не нужно ничего настраивать вручную!).

---

## ✨ Особенности (Features)

- 🚀 **100% Rust & Vulkan**: Лаунчер и сам игровой движок написаны на Rust. Никакой медленной Java во время игры, мгновенный старт и сотни кадров в секунду (500+ FPS на современных видеокартах).
- 🔓 **Без лицензии (Offline Mode)**: Вводите любой желаемый никнейм. Не требует логина Microsoft, паролей или проверки владения игрой.
- 📦 **Версия «из коробки» (Pre-installed)**: Скомпилированные ресурсы и ассеты Minecraft 26.3 уже подготовлены. Нажали «ИГРАТЬ» — и сразу в мире!
- 🎨 **Красивый и удобный интерфейс (egui)**: Минималистичный темный геймерский дизайн с изумрудными акцентами, карточками и светящейся кнопкой старта.
- ⚙️ **Гибкие настройки**:
  - Выделение оперативной памяти (от 2 до 16 GB).
  - Настройка дальности прорисовки (от 4 до 32 чанков).
  - Выбор разрешения окна (1280x720, 1600x900, 1920x1080) и полноэкранный режим.
  - Режим игры по умолчанию (Выживание / Творческий).
- 📁 **Быстрый доступ к файлам**: Кнопки открытия папок сохранений (`saves`), скриншотов (`screenshots`) и корневой директории игры в один клик.
- 📋 **Встроенная консоль**: Мониторинг логов и вывода Vulkan-рендерера в реальном времени.

---

## 🎮 Как запустить

### Вариант 1: Запуск готового бинарника (Портативный)
1. Скачайте архив релиза `Horizon-Launcher-v1.0.0-Portable.zip`.
2. Распакуйте в любое удобное место.
3. Запустите `start_launcher.bat` или дважды кликните по `horizon-offline-launcher.exe`.
4. Введите ник и нажмите большую зеленую кнопку **«ИГРАТЬ»**!

---

## 🛠 Сборка из исходников (Building from Source)

Требуется установленный Rust (`cargo` / `rustc` 1.80+).

```powershell
# Клонировать репозиторий
git clone https://github.com/e04059631-lab/horizon-offline-launcher.git
cd horizon-offline-launcher

# Собрать релизную версию
cargo build --release

# Запустить лаунчер
.\target\release\horizon-offline-launcher.exe
```

---

## 📂 Структура проекта

```text
horizon-offline-launcher/
├── src/
│   ├── main.rs          # Точка входа и окно eframe/egui
│   ├── config.rs        # Управление настройками (launcher_config.json)
│   ├── game_process.rs  # Запуск и мониторинг процесса horizon-client
│   ├── ui.rs            # Графический интерфейс и вкладки
│   └── theme.rs         # Стилизация и цвета темного интерфейса
├── game/                # Встроенный клиент и ресурсы 26.3
│   ├── horizon-client.exe
│   └── .local/horizon-client/26.3/
├── build_release.ps1    # Скрипт релизной компиляции
├── package_release.ps1  # Скрипт сборки ZIP-дистрибутива
└── Cargo.toml
```

---

## ⚖️ Правовая информация

Horizon — это независимый фанатский проект с открытым исходным кодом. Minecraft является зарегистрированным товарным знаком Mojang Studios и Microsoft Corporation. Лаунчер не связан с Mojang Studios или Microsoft.

Лицензия: [MIT](LICENSE).
