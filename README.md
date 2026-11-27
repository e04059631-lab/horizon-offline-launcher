<p align="center">
  <img src="assets/banner.jpg" alt="Horizon Offline Launcher Banner" width="100%">
</p>

<h1 align="center">Horizon Offline Launcher</h1>

<p align="center">
  <strong>Автономный оффлайн-лаунчер для Minecraft Horizon 26.3 на Rust + Slint со встроенными ассетами, адаптивной темой и онлайн-каталогом шейдеров и PvP-паков.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/UI-Slint%201.18-0A84FF?style=for-the-badge" alt="Slint UI">
  <img src="https://img.shields.io/badge/Mode-No%20License%20%2F%20Offline-34C759?style=for-the-badge" alt="Offline Mode">
  <img src="https://img.shields.io/badge/Minecraft-Horizon%2026.3-FF6A1F?style=for-the-badge" alt="Horizon 26.3">
  <img src="https://img.shields.io/badge/Platform-Windows%20x64-4E73DF?style=for-the-badge" alt="Windows">
</p>

---

## ✨ Возможности

* 🔓 **Полностью без лицензии (Offline Mode)**  
  Без аккаунтов Microsoft и онлайн-авторизации — достаточно ввести любой никнейм и нажать «Играть».
* 📦 **Все ассеты встроены в один `.exe` (Zero-Download)**  
  Клиент Horizon 26.3 и все 28 000+ ресурсов упакованы внутрь одного исполняемого файла. При первом запуске лаунчер автоматически распаковывает клиент — можно играть полностью оффлайн.
* 🎨 **Живые фоны и адаптивный акцент**  
  4 игровых фона Minecraft с шейдерами + выбор собственного изображения («Свой фон»). Акцентный цвет всего интерфейса — кнопки, слайдеры, переключатели и сама иконка **H** (в шапке, панели запуска и на панели задач Windows) — автоматически подстраивается под выбранные обои. Поддерживаются тёмная и светлая темы.
* 🧩 **Онлайн-каталог на 56 клиентских дополнений**  
  Полноширинная вкладка «Моды» с переключателем **Каталог / Мои**, поиском и фильтрами (`Все`, `Шейдеры`, `PvP-паки`):
  * **27 нативных WGSL-шейдеров** (`.hshader`) с поддержкой объединения нескольких шейдеров одновременно;
  * **29 клиентских PvP и QoL ресурс-паков** (`.zip`), работающих и в одиночной игре, и на любом сервере.
* ⚙️ **Удобные настройки и консоль**  
  Пресеты производительности (`Эконом` / `Баланс` / `Ультра`), настройка ОЗУ и дальности прорисовки, выбор разрешения, полноэкранный и творческий режимы, быстрый вход по IP сервера и живая консоль логов с фильтрацией.

---

## 🎮 Скачать (Releases)

Готовый автономный `.exe` со всеми встроенными ассетами Horizon 26.3 доступен на странице релизов:

👉 **[Перейти к релизам Horizon Offline Launcher](https://github.com/xwwerz/horizon-offline-launcher/releases)**

| Файл | Платформа | Размер | Описание |
| :--- | :--- | :--- | :--- |
| **`horizon-offline-launcher.exe`** | Windows 10 / 11 (x64) | ~392 МБ | Полностью автономный портативный `.exe` со встроенным клиентом и ассетами 26.3 |

---

## 🧩 Каталог модов

Официальный репозиторий шейдеров и PvP-паков: **[xwwerz/horizon-mods](https://github.com/xwwerz/horizon-mods)**  
Адрес каталога можно в любой момент изменить прямо в лаунчере (**Настройки → Каталог и файлы**).

---

## 🛠 Сборка из исходников

```bash
git clone https://github.com/xwwerz/horizon-offline-launcher.git
cd horizon-offline-launcher
cargo build --release
```

Для упаковки собранного бинарника вместе с архивом ассетов (`HORIZON_PAYLOAD`) используется скрипт `pack_release.ps1`.

---

## ⚖️ Лицензия

Исходный код распространяется под лицензией [MIT](LICENSE).  
Minecraft является зарегистрированной торговой маркой Mojang Studios / Microsoft. Проект является независимым фанатским форком.
