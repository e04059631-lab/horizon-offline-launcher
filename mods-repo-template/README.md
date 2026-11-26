# Репозиторий модов для Horizon Offline Launcher (GitHub)

Эта папка — готовый шаблон для твоего публичного GitHub-репозитория модов, который работает **24/7 по всему миру** без необходимости держать включённым свой ПК.

## Быстрая настройка за 2 минуты

1. Создай новый **публичный (Public)** репозиторий на GitHub (например, `horizon-mods`).
2. Залей туда:
   - `catalog.json` (из этой папки)
   - папку `mods/` со своими файлами модов (`.hmod`, `.hnmod`, `.jar`, `.zip`)
   - папку `icons/` с квадратными PNG/JPG иконками модов (например, `128×128` или `256×256`)
3. В лаунчере открой **Настройки → Файлы клиента** и вставь ссылку на свой репозиторий, например:
   ```text
   https://github.com/ТВОЙ_НИК/horizon-mods
   ```
   *(лаунчер сам преобразует её в `https://raw.githubusercontent.com/ТВОЙ_НИК/horizon-mods/main/catalog.json`)*

## Как добавить новый мод

1. Закинь файл мода в папку `mods/` (или прикрепи к **GitHub Release**, если файл больше 25 МБ).
2. Закинь картинку в `icons/` (необязательно — если оставить `"icon_url": ""`, лаунчер покажет стандартную иконку).
3. Добавь объект в массив `"mods"` в файле `catalog.json`:

```json
{
  "id": "my-cool-mod",
  "name": "Название мода",
  "description": "Краткое описание, что делает мод",
  "version": "1.0.0",
  "file_name": "my-cool-mod.hmod",
  "download_url": "mods/my-cool-mod.hmod",
  "icon_url": "icons/my-cool-mod.png",
  "size_kb": 640
}
```

> **Совет для тяжёлых модов (> 25 МБ):**
> В `download_url` можно указать любую полную прямую ссылку, например на GitHub Releases:
> `"download_url": "https://github.com/ТВОЙ_НИК/horizon-mods/releases/download/v1.0/big-mod.hmod"`
