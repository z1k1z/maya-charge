#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
if (( EUID == 0 )); then
    echo 'Запусти bash install.sh от обычного пользователя, без sudo.' >&2
    exit 1
fi
for tool in cargo python noctalia; do
    command -v "$tool" >/dev/null || { echo "Не найден $tool" >&2; exit 1; }
done
cargo build --release --locked --offline
noctalia plugins lint ./noctalia
python scripts/configure.py

# Updates do not need sudo or another udev trigger when the rule is unchanged.
if ! cmp -s udev/70-lamzu-maya-battery.rules /etc/udev/rules.d/70-lamzu-maya-battery.rules; then
    sudo install -m 0644 udev/70-lamzu-maya-battery.rules /etc/udev/rules.d/70-lamzu-maya-battery.rules
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=hidraw --action=change
    sudo udevadm settle
fi

plugin_dir="${XDG_DATA_HOME:-$HOME/.local/share}/noctalia/plugins/mouse-charge"
mkdir -p -- "$plugin_dir" "$HOME/.local/bin"
install -m 0755 target/release/mouse-charge "$plugin_dir/mouse-charge.new"
mv -f -- "$plugin_dir/mouse-charge.new" "$plugin_dir/mouse-charge"
install -m 0755 target/release/mouse-charge "$HOME/.local/bin/mouse-charge.new"
mv -f -- "$HOME/.local/bin/mouse-charge.new" "$HOME/.local/bin/mouse-charge"
for file in plugin.toml widget.luau service.luau; do
    install -m 0644 "noctalia/$file" "$plugin_dir/$file.new"
    mv -f -- "$plugin_dir/$file.new" "$plugin_dir/$file"
done
python scripts/configure.py --apply
noctalia msg config-reload
noctalia msg plugins enable local/mouse-charge
noctalia msg config-reload
printf '\nУстановлено: опрос каждые 5 секунд, запуск вместе с Noctalia.\n'
if ! "$HOME/.local/bin/mouse-charge" --diagnose; then
    printf '\nПока нет данных от мыши. Переподключи ресивер, разбуди мышь и нажми на индикатор.\n' >&2
fi
