# Vatra

A native Rust dashboard for Linux laptop power and CPU telemetry. Vatra opens
an ordinary desktop window using winit, softbuffer, and tiny-skia.

## Try the live dashboard

On Debian 13 or an MX Linux release based on Debian 13, install the build tools
and Fontconfig. Vatra uses the system sans-serif font:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev fontconfig
```

Use a current stable Rust toolchain. From this directory:

```sh
cargo test
cargo run --release --locked
```

The first Cargo command resolves dependencies and generates Cargo.lock.
Keep Cargo.lock with the source when committing this version.

## Install or uninstall

Run `bash install.sh` as your regular user. Open **Vatra** from the
application menu or run `~/.local/bin/vatra`. Launching it again signals
the running window. On Wayland, the compositor controls whether it can be
raised or highlighted. Rerun the installer after
updating the source. Run `bash uninstall.sh` to remove the launcher and
executable. History and the optional system helper can be removed separately.

## Build a Debian package

On Debian 13 or MX Linux based on Debian 13, install `dpkg-dev` and
`desktop-file-utils` in addition to the build dependencies above. From
`common/vatra`:

```sh
bash packaging/build-deb.sh
sudo apt install ./dist/vatra_0.1.0_$(dpkg --print-architecture).deb
/usr/bin/vatra
```

The package installs a system-wide launcher and the optional TLP helper.
For TLP controls, install TLP 1.8, Python 3, and pkexec, and run a graphical
polkit agent. Installation does not change your TLP configuration or
enable services. If switching from `install.sh` to the package, run
`bash uninstall.sh` to remove the user launcher, which would otherwise
take precedence over the packaged launcher.

The GitHub Actions workflow runs the Rust tests on Debian 13 and uploads
the package as a build artifact. Commit `Cargo.lock` with the source so that
local and CI builds use the same dependency versions.

## Readings

Vatra reads Linux sysfs every two seconds while the window is open. It selects
the first present system battery by sysfs name and summarizes the available CPU
frequency policies. Missing sensor data appears as a dash or as unavailable.
Charge start and stop thresholds are shown when the battery driver exposes
them. Vatra displays these thresholds as read-only telemetry.

Battery power comes from the driver or is calculated from voltage and current.
The chart uses positive values for charging and negative values for discharging,
with a rolling chart. Drag the range slider or use Left/Right in Overview
to show the last 1–10 minutes. Recent readings are saved every 30 seconds
and on normal exit to `$XDG_STATE_HOME/vatra/battery-power.tsv` (by default,
`~/.local/state/vatra/battery-power.tsv`). The chart leaves gaps for times
when Vatra was closed or no sample was available. The runtime estimate uses the
latest discharge sample and can vary with load. These values describe the
battery. CPU package power and power at the wall require separate measurements.

Monitoring and profile previews run without root and do not require TLP.

Below the chart, **Battery discharge** shows the average, lowest and highest
observed draw in watts over the last 1, 5 and 10 minutes. Each window uses the
available discharge samples. Charging and missing readings are skipped, and an
empty window shows dashes. The chart range slider does not change these windows.

## Power modes

Open **Power modes** and choose a mode. **Automatic** uses balanced performance
on AC and `balance_power` on battery. TLP switches between them as the power
source changes, including when Vatra is closed. **Balanced**, **Performance**,
and **Power Saver** use the same preference for AC and battery. Power Saver
prefers `power` and falls back to `balance_power` if the CPU policies do not
offer `power`. Mode buttons request authorization when there is a change.

In **Advanced**, choose AC or BAT and a preset. Edit its energy preference
and maximum CPU speed separately. **Reset limit** restores the selected
preset's ceiling. **Custom** keeps the current CPU ceiling until you set one.
These edits save to `$XDG_CONFIG_HOME/vatra/profiles.conf` (normally
`~/.config/vatra/profiles.conf`). **Review changes** applies the selected
preset to the active power source. Selecting a quick mode in **Power modes**
uses the saved AC and BAT settings together.

When the page opens, Vatra checks standard TLP executable paths and known
systemd power manager units. An unavailable systemd query is reported as
unknown. For TLP releases that report AC and BAT keys, Vatra reads effective
EPP and maximum frequency from `tlp-stat -c` and shows the source filenames.
These are configured values. The live CPU readings above are from sysfs.
For TLP 1.8, the preview also compares the power source and mode, configured
EPP, and live EPP. Vatra refreshes Profiles when Linux reports a change in
external power. TLP still has to confirm the active source before Apply is
available. Select **Refresh** or press R to refresh manually and follow the
active source.

## Apply CPU settings through TLP 1.8

On Debian 13, the optional helper requires Python 3, TLP 1.8, systemd and
pkexec with a graphical polkit agent. The Debian package includes the helper.
For a source install, install it separately as administrator from this
directory, then launch Vatra as your regular user.

```sh
sudo install -D -o root -g root -m 0755 packaging/vatra-tlp-helper \
    /usr/local/libexec/vatra-tlp-helper
cargo run --release --locked
```

Review the current and proposed values before applying. When a plan raises
the current limit or exceeds a mode's default, select **Raise and apply** to
confirm. Polkit asks for administrator authorization. The helper checks the
source, live CPU policies, and known power managers again. It writes the
selected settings to `/etc/tlp.d/99-vatra-epp.conf` and checks the effective
configuration. It runs `tlp start` when live settings need updating.
It verifies EPP, governor and the live CPU frequency limits afterward. If a
check fails, it attempts to restore the previous override and reapply the
previous TLP configuration if necessary.

To remove the helper, delete /usr/local/libexec/vatra-tlp-helper. To also
restore the underlying TLP configuration, remove Vatra's 99-vatra-epp.conf
drop-in and apply TLP again. Existing TLP drop-ins remain in place.

The quick modes also set a CPU frequency ceiling via TLP. Automatic and
Performance use the full available range. Balanced and Power Saver use 80%
and 60% of the range above the minimum, respectively. An existing lower TLP
ceiling defines the maximum usable range. Vatra preserves the existing
governor and minimum frequency; all modes still let the CPU scale with load.
Manual limits must be within the supported range and above the configured minimum.


## XFCE tray (X11)

For tray support, install `python3-gi` and `gir1.2-gtk-3.0` and make sure
the XFCE panel has a Notification Area. Click **Show in tray** in Overview
or run `~/.local/bin/vatra --enable-tray`. The tray preference is saved for
the next launch. Close the window to keep Vatra in the tray; the tray menu
has **Quit Vatra** and **Disable tray**.

Left-click the indicator to open Overview. Its tooltip shows charge,
battery state, estimated runtime when available, and the active mode.
Right-click to switch power modes through the usual TLP authorization or
enable **Start at login**. Login startup and tray visibility are separate
preferences. If the indicator cannot attach to the panel, Vatra opens its
window. The tray is available on X11.

## Vatra Panel for XFCE

Vatra Panel is an optional XFCE panel plugin. It displays a battery icon from
the system theme and the percentage as regular panel text. It listens for
UPower battery updates and opens the Vatra dashboard when clicked.

Install the main Vatra Debian package first. On Debian 13 or MX Linux based
on Debian 13, run these commands from `common/vatra`:

```sh
sudo apt install build-essential pkg-config dpkg-dev libxfce4panel-2.0-dev libgtk-3-dev
bash packaging/build-xfce-panel-deb.sh
sudo apt install "./dist/vatra-xfce-panel_0.1.0_$(dpkg --print-architecture).deb"
```

Open **Panel → Add New Items** in XFCE and add **Vatra Panel**. If you want
a single Vatra indicator, choose **Disable tray** from Vatra's compact tray
menu. Remove the plugin from the panel and run
`sudo apt remove vatra-xfce-panel` to uninstall it.

## Review power changes

Power modes and Advanced show the current and proposed maximum CPU
frequency and EPP before applying. The active profile uses live values;
the inactive profile shows the saved TLP configuration. Selecting a mode
from the tray opens the review window. Selecting Cancel makes no changes.
If the measured range or TLP configuration changes before authorization,
review the plan again.

## Sidebar and appearance

Use the sidebar to navigate Overview, Power modes, Advanced, Diagnostics,
Appearance and About. Appearance saves the accent color for the graph,
normal battery fill, selection highlights and the tray battery icon.
Pick a preset or choose a custom color on the wheel. The XFCE tray can put
the battery percentage inside the icon, beside it or in the tooltip only.
The charging bolt can match the accent, use white or be hidden.
