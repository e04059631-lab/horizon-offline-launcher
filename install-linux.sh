#!/usr/bin/env bash
set -euo pipefail

REPO="xwwerz/horizon-offline-launcher"
ASSET_NAME="horizon-offline-launcher-linux-x86_64"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ASSET_NAME}"
INSTALL_BIN="${HOME}/.local/bin/horizon-launcher"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

info()  { printf "${BLUE}[Horizon]${NC} %s\n" "$*"; }
ok()    { printf "${GREEN}[OK]${NC} %s\n" "$*"; }
warn()  { printf "${YELLOW}[!]${NC} %s\n" "$*"; }
err()   { printf "${RED}[Ошибка]${NC} %s\n" "$*" >&2; exit 1; }

if [[ "${1:-}" == "--uninstall" || "${1:-}" == "uninstall" ]]; then
    if [[ -x "${INSTALL_BIN}" ]]; then
        "${INSTALL_BIN}" --uninstall "${2:-}"
        exit 0
    fi
    rm -f "${INSTALL_BIN}" \
          "${HOME}/.local/share/applications/horizon-launcher.desktop" \
          "${HOME}/.local/share/icons/hicolor/256x256/apps/horizon-launcher.png"
    ok "Horizon Offline Launcher удалён. Миры сохранены в ~/.local/share/horizon-offline-game"
    exit 0
fi

OS="$(uname -s)"
ARCH="$(uname -m)"
if [[ "${OS}" != "Linux" ]]; then
    err "Этот скрипт предназначен для Linux (обнаружено: ${OS})."
fi
if [[ "${ARCH}" != "x86_64" && "${ARCH}" != "amd64" ]]; then
    err "Поддерживается только архитектура x86_64 (обнаружено: ${ARCH})."
fi

echo ""
printf "${BOLD}Horizon Offline Launcher — установка на Linux (x86_64)${NC}\n"
echo "Включает встроенный клиент Horizon 26.3, все ассеты (~393 МБ) и CLI-команды."
echo ""

TMP_DIR="$(mktemp -d)"
TMP_BIN="${TMP_DIR}/${ASSET_NAME}"
cleanup() { rm -rf "${TMP_DIR}"; }
trap cleanup EXIT

info "Скачивание ${ASSET_NAME} с GitHub Releases..."
if command -v curl >/dev/null 2>&1; then
    curl -fL --progress-bar "${DOWNLOAD_URL}" -o "${TMP_BIN}"
elif command -v wget >/dev/null 2>&1; then
    wget --show-progress -qO "${TMP_BIN}" "${DOWNLOAD_URL}"
else
    err "Не найден curl или wget. Установите curl и повторите попытку."
fi

chmod +x "${TMP_BIN}"

info "Установка в ~/.local/bin и регистрация ярлыка в меню приложений..."
"${TMP_BIN}" --install

echo ""
ok "Установка завершена!"
echo ""
echo "  Запуск лаунчера:      horizon-launcher"
echo "  Быстрый вход в игру:  horizon-launcher --play ТвойНик"
echo "  Все команды CLI:      horizon-launcher --help"
echo "  Удаление:             horizon-launcher --uninstall"
echo ""

if [[ ":${PATH}:" != *":${HOME}/.local/bin:"* ]]; then
    warn "~/.local/bin не добавлен в PATH в текущей сессии."
    echo "  Добавьте в ~/.bashrc или ~/.zshrc:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo "  Или запускайте из меню приложений / командой: ~/.local/bin/horizon-launcher"
    echo ""
fi
