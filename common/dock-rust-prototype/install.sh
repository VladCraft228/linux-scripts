#!/usr/bin/env bash
set -euo pipefail
SOURCE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
[[ $EUID -ne 0 ]] || { echo 'Run as your regular user, without sudo.' >&2; exit 1; }
AUTOSTART=true
BUILD=true
for option in "$@"; do
    case "$option" in
        --no-autostart) AUTOSTART=false ;;
        --skip-build) BUILD=false ;;
        --help) echo 'Usage: bash install.sh [--no-autostart] [--skip-build]'; exit 0 ;;
        *) echo "Unknown option: $option" >&2; exit 1 ;;
    esac
done
if $BUILD; then bash "$SOURCE_DIR/build.sh"; fi
SOURCE_BINARY="$SOURCE_DIR/target/release/prychal-dock"
[[ -x $SOURCE_BINARY ]] || { echo 'No release binary. Run bash build.sh first.' >&2; exit 1; }
TARGET="$HOME/.local/bin/prychal-dock"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
APPLICATIONS_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/scalable/apps"
TEMP_FILES=()
cleanup() { if ((${#TEMP_FILES[@]})); then rm -f -- "${TEMP_FILES[@]}"; fi; }
trap cleanup EXIT
mkdir -p -- "$(dirname -- "$TARGET")" "$APPLICATIONS_DIR" "$ICON_DIR"
# Desktop Entry string decoding precedes Exec argument decoding. Preserve both
# layers, including literal percent signs, without running a shell command.
exec_argument() {
    local input=$1 output='' char i
    for ((i=0; i<${#input}; i++)); do
        char=${input:i:1}
        case "$char" in
            \\) output+='\\\\' ;;
            '"') output+='\\\"' ;;
            '$') output+='\\$' ;;
            '`') output+='\\`' ;;
            '%') output+='%%' ;;
            $'\n'|$'\r') echo 'Newlines in the installation path are unsupported.' >&2; return 1 ;;
            *) output+="$char" ;;
        esac
    done
    printf '%s' "$output"
}
ESCAPED=$(exec_argument "$TARGET")
TEMP=$(mktemp "${TARGET}.XXXXXX")
TEMP_FILES+=("$TEMP")
install -m 755 -- "$SOURCE_BINARY" "$TEMP"
mv -f -- "$TEMP" "$TARGET"
TEMP=$(mktemp "$ICON_DIR/.prychal-dock.XXXXXX")
TEMP_FILES+=("$TEMP")
install -m 644 -- "$SOURCE_DIR/assets/prychal-dock.svg" "$TEMP"
mv -f -- "$TEMP" "$ICON_DIR/prychal-dock.svg"
TEMP=$(mktemp "$APPLICATIONS_DIR/.prychal-dock.XXXXXX")
TEMP_FILES+=("$TEMP")
cat > "$TEMP" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Prychal Dock
Comment=Customizable application dock for XFCE on X11
Exec="$ESCAPED"
Icon=prychal-dock
Terminal=false
StartupNotify=false
Categories=Utility;
StartupWMClass=prychal-dock
Actions=Settings;Quit;

[Desktop Action Settings]
Name=Dock Settings
Exec="$ESCAPED" --settings

[Desktop Action Quit]
Name=Quit Dock
Exec="$ESCAPED" --quit
DESKTOP
chmod 644 "$TEMP"
mv -f -- "$TEMP" "$APPLICATIONS_DIR/prychal-dock.desktop"
LEGACY_ESCAPED=$(exec_argument "$HOME/.local/bin/linux-scripts-dock-rust")
LEGACY_ENTRY="$CONFIG_DIR/autostart/linux-scripts-dock-rust.desktop"
ENTRY="$CONFIG_DIR/autostart/prychal-dock.desktop"
if [[ -e $LEGACY_ENTRY ]]; then
    if [[ ! -e $ENTRY && ! -L $ENTRY ]]; then
        mkdir -p -- "$CONFIG_DIR/autostart"
        TEMP=$(mktemp "$CONFIG_DIR/autostart/.prychal-migrate.XXXXXX")
        TEMP_FILES+=("$TEMP")
        while IFS= read -r line || [[ -n $line ]]; do
            case "$line" in
                Exec=*)
                    OLD_TOKEN="\"$LEGACY_ESCAPED\""
                    NEW_TOKEN="\"$ESCAPED\""
                    line=${line/"$OLD_TOKEN"/"$NEW_TOKEN"}
                    case "$line" in
                        Exec=linux-scripts-dock-rust|Exec=linux-scripts-dock-rust\ *)
                            line="Exec=prychal-dock${line#Exec=linux-scripts-dock-rust}" ;;
                    esac
                    ;;
                'Name=Linux Scripts Dock (Rust Edition)') line='Name=Prychal Dock' ;;
                'Icon=linux-scripts-dock-rust') line='Icon=prychal-dock' ;;
            esac
            printf '%s\n' "$line" >> "$TEMP"
        done < "$LEGACY_ENTRY"
        chmod 644 "$TEMP"
        mv -- "$TEMP" "$ENTRY"
        echo "Autostart migrated, including enabled/disabled state: $ENTRY"
    fi
    BACKUP_DIR="$CONFIG_DIR/linux-scripts-dock-rust-prototype/migration-backups"
    mkdir -p -- "$BACKUP_DIR"
    BACKUP=$(mktemp "$BACKUP_DIR/rust-autostart.XXXXXX")
    mv -f -- "$LEGACY_ENTRY" "$BACKUP"
    echo "Old autostart backup: $BACKUP"
fi
# Keep the former command working for existing scripts and imported shortcuts.
LEGACY_TARGET="$HOME/.local/bin/linux-scripts-dock-rust"
TEMP=$(mktemp "${LEGACY_TARGET}.XXXXXX")
TEMP_FILES+=("$TEMP")
rm -f -- "$TEMP"
ln -s prychal-dock "$TEMP"
mv -f -- "$TEMP" "$LEGACY_TARGET"
rm -f -- "$APPLICATIONS_DIR/linux-scripts-dock-rust.desktop" "$ICON_DIR/linux-scripts-dock-rust.svg"
if $AUTOSTART; then
    mkdir -p -- "$CONFIG_DIR/autostart"
    ENTRY="$CONFIG_DIR/autostart/prychal-dock.desktop"
    if [[ -e $ENTRY || -L $ENTRY ]]; then
        echo "Existing autostart entry preserved: $ENTRY"
    else
        TEMP=$(mktemp "$CONFIG_DIR/autostart/.prychal-dock.XXXXXX")
        TEMP_FILES+=("$TEMP")
        cat > "$TEMP" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Prychal Dock
Comment=Application dock for XFCE on X11
Exec="$ESCAPED"
Icon=prychal-dock
Terminal=false
StartupNotify=false
OnlyShowIn=XFCE;
DESKTOP
        chmod 644 "$TEMP"
        mv -- "$TEMP" "$ENTRY"
        echo "Autostart created: $ENTRY"
    fi
else
    echo 'Autostart creation skipped; any existing entry was preserved.'
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPLICATIONS_DIR" || true
fi
printf 'Installed: %s\nApplication launcher: %s\n' "$TARGET" "$APPLICATIONS_DIR/prychal-dock.desktop"
echo 'Settings and pinned applications were preserved.'
echo 'Quit any running Prychal Dock before starting the updated binary.'
printf 'Run: "%s"\n' "$TARGET"
echo 'The Qt Dock and its autostart were not modified.'

if [[ -e "$CONFIG_DIR/autostart/linux-scripts-dock.desktop" ]]; then
    echo 'A Qt Dock autostart entry also exists. Choose one edition in XFCE Session and Startup.'
fi
