# SPDX-License-Identifier: GPL-3.0-only

# Name of the application's binary.
name := 'magnetar-accounts'
# The unique ID of the application.
appid := 'com.magnetaros.Accounts'

# Path to root file system, which defaults to `/`.
rootdir := ''
# The prefix for the `/usr` directory.
prefix := '/usr'
# The location of the cargo target directory.
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')

# Sources. Named after the IDs they install as, because desktop-file-validate
# and appstreamcli both check the file name against the component ID. Static
# files, checked in as the shipping COSMIC applications do it, not generated
# by a build script: `xdgen` as a build dependency re-resolves the graph and
# drops zbus's `blocking` feature, which single-instance needs.
desktop-src := 'resources' / (appid + '.desktop')
# The two autostart entries: the welcome at a new user's first login, and the
# background link to GNOME Online Accounts.
first-run-src := 'resources' / (appid + '.FirstRun.desktop')
watch-src := 'resources' / (appid + '.Watch.desktop')
metainfo-src := 'resources' / (appid + '.metainfo.xml')
icon-dir := 'resources' / 'icons' / 'hicolor'
icon-src := icon-dir / 'scalable' / 'apps' / (appid + '.svg')
icon-symbolic-src := icon-dir / 'symbolic' / 'apps' / (appid + '-symbolic.svg')
# The scalable SVG is what modern toolkits pick up; the PNGs are rasterised
# from it at each size so the panel and the icon grid get pixel-exact art.
icon-sizes := '16x16 24x24 32x32 48x48 64x64 128x128 256x256 512x512'

# Install destinations
base-dir := absolute_path(clean(rootdir / prefix))
bin-dst := base-dir / 'bin' / name
desktop-dst := base-dir / 'share' / 'applications' / (appid + '.desktop')
# In /etc/xdg/autostart, where the session looks, as grabit's entry is; the
# package carries /etc as a second tree for them (packaging/linux/nfpm.yaml).
# A per-user install puts them where a per-user session looks instead.
autostart-dir := if prefix == '/usr' { absolute_path(clean(rootdir / 'etc')) / 'xdg' / 'autostart' } else { config_directory() / 'autostart' }
first-run-dst := autostart-dir / (appid + '.FirstRun.desktop')
watch-dst := autostart-dir / (appid + '.Watch.desktop')
metainfo-dst := base-dir / 'share' / 'metainfo' / (appid + '.metainfo.xml')
icons-dst := base-dir / 'share' / 'icons' / 'hicolor'
icon-svg-dst := icons-dst / 'scalable' / 'apps' / (appid + '.svg')
icon-symbolic-dst := icons-dst / 'symbolic' / 'apps' / (appid + '-symbolic.svg')

# Default recipe which runs `just build-release`
default: build-release

# Everything CI runs, in the order that fails cheapest first
check-all: validate-metadata fmt-check check test

# Runs `cargo clean`
clean:
    cargo clean

# Removes vendored dependencies
clean-vendor:
    rm -rf .cargo vendor vendor.tar

# `cargo clean` and removes vendored dependencies
clean-dist: clean clean-vendor

# Compiles with debug profile
build-debug *args:
    cargo build --locked {{args}}

# Compiles with release profile
build-release *args: (build-debug '--release' args)

# Compiles release profile with vendored dependencies
build-vendored *args: vendor-extract (build-release '--frozen --offline' args)

# Runs a clippy check
check *args:
    cargo clippy --all-features --all-targets --locked {{args}} -- -D warnings

# Runs a clippy check with JSON message format
check-json: (check '--message-format=json')

# Checks formatting without rewriting anything
fmt-check:
    cargo fmt --all -- --check

# Rewrites formatting
fmt:
    cargo fmt --all

# Runs the test suite. The Online Accounts tests start a private
# `dbus-daemon`, which must be installed.
test *args:
    cargo test --locked {{args}}

# Checks the desktop entries and AppStream metadata against their specs
#
# Offline: the metainfo names a remote icon on `main`, which appstreamcli cannot
# fetch from a branch that has not been merged yet.
validate-metadata:
    #!/usr/bin/env bash
    set -uo pipefail
    # `Categories=COSMIC;` is what COSMIC's own apps and applets ship, but
    # desktop-file-validate 0.27 (Ubuntu noble, which CI has) rejects the
    # unregistered value as an error; 0.28 downgraded it to a hint. Matching
    # the platform matters more than the older validator, so that one error is
    # tolerated and every other error still fails the recipe. Same rule as
    # circle's, slate's and pencil's `validate-metadata`.
    for entry in {{desktop-src}} {{first-run-src}} {{watch-src}}; do
        output=$(desktop-file-validate "$entry" 2>&1 || true)
        [ -n "$output" ] && printf '%s\n' "$output"
        if printf '%s\n' "$output" | grep 'error:' | grep -qv 'unregistered value "COSMIC"'; then
            echo "validate-metadata: $entry failed validation" >&2
            exit 1
        fi
    done
    # `DBusActivatable=true` makes GIO launchers call org.freedesktop.Application
    # on the application's bus name and never run Exec. libcosmic's single
    # instance serves no such interface, so with the key set those launchers
    # start nothing; desktop-file-validate accepts it all the same.
    if grep -q '^DBusActivatable=true' {{desktop-src}}; then
        echo 'validate-metadata: the desktop entry claims D-Bus activation, which nothing serves' >&2
        exit 1
    fi
    appstreamcli validate --no-net {{metainfo-src}}

# Also checks that the remote icon and URLs actually resolve
validate-metadata-urls:
    appstreamcli validate {{metainfo-src}}

# Run the application for testing purposes
run *args:
    env RUST_LOG=magnetar_accounts=debug RUST_BACKTRACE=full cargo run --release -- {{args}}

# Run against a scratch account list, leaving the real one alone. The
# keychain is skipped too: secrets land in an encrypted file beside the list.
run-sandboxed dir='/tmp/magnetar-accounts' *args: build-release
    mkdir -p {{dir}}
    env COSMIC_PIM_CONFIG_DIR={{dir}} COSMIC_PIM_NO_KEYRING=1 RUST_BACKTRACE=full {{ cargo-target-dir / 'release' / name }} {{args}}

# Installs the application
#
# The two cache refreshes are guarded on rootdir: run unguarded they create
# mimeinfo.cache / icon-theme.cache inside a staged package root, and those
# cache files then ship in the package and conflict with every other
# package's copy. A staged tree belongs to a package manager whose hooks
# refresh the real caches at install time.
install:
    install -Dm0755 {{ cargo-target-dir / 'release' / name }} {{bin-dst}}
    install -Dm0644 {{desktop-src}} {{desktop-dst}}
    install -Dm0644 {{first-run-src}} {{first-run-dst}}
    install -Dm0644 {{watch-src}} {{watch-dst}}
    install -Dm0644 {{metainfo-src}} {{metainfo-dst}}
    install -Dm0644 {{icon-src}} {{icon-svg-dst}}
    install -Dm0644 {{icon-symbolic-src}} {{icon-symbolic-dst}}
    for size in {{icon-sizes}}; do \
        install -Dm0644 {{icon-dir}}/$size/apps/{{appid}}.png \
            {{icons-dst}}/$size/apps/{{appid}}.png; \
    done
    if [ -z '{{rootdir}}' ]; then \
        update-desktop-database {{ base-dir / 'share' / 'applications' }} || true; \
        gtk-update-icon-cache -t {{icons-dst}} || true; \
    fi

# Uninstalls installed files
uninstall:
    rm -f {{bin-dst}} {{desktop-dst}} {{first-run-dst}} {{watch-dst}} {{metainfo-dst}} {{icon-svg-dst}} {{icon-symbolic-dst}}
    for size in {{icon-sizes}}; do \
        rm -f {{icons-dst}}/$size/apps/{{appid}}.png; \
    done

# Installs into the current user's home, no root needed. Handy for trying it out.
install-user:
    just rootdir='' prefix={{home_directory()}}/.local install

# Vendor dependencies locally
vendor:
    #!/usr/bin/env bash
    mkdir -p .cargo
    cargo vendor --sync Cargo.toml | head -n -1 > .cargo/config.toml
    echo 'directory = "vendor"' >> .cargo/config.toml
    tar pcf vendor.tar vendor
    rm -rf vendor

# Extracts vendored dependencies
vendor-extract:
    rm -rf vendor
    tar pxf vendor.tar
