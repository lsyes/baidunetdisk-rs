baidunetdisk-rs

A third-party Baidu Netdisk (百度网盘) desktop client for Linux, implemented entirely in Rust.

baidunetdisk-rs is built on top of kasumi, a pure-Rust immediate-mode GUI framework, with the wgpu rendering backend. It provides a native Linux experience for managing your Baidu Netdisk files without relying on any proprietary or web-based runtime.

Features

· File management — browse, search, create directories, rename, move, copy, and delete files in your Netdisk.
· Multi-connection chunked downloads — resumable transfers with pause and concurrent queue support.
· Chunked uploads — full upload pipeline via precreate / superfile2 / commit.
· Share links — create, parse, and transfer ("save to my disk") share links.
· Built-in preview — text, Markdown, images, and PDF (PDF rendering via the pure-Rust hayro engine).
· Combined detail view — inspect file metadata and details in a single panel.
· Flexible login — QR-code scan login or manual Cookie login.

Installation

From source

Ensure you have a Rust toolchain (rustc and cargo >= 1.88) plus the build dependencies:

```sh
sudo apt install pkg-config libssl-dev
```

Then clone and build:

```sh
git clone https://github.com/lsyes/kasumi.git
cd kasumi
cargo build --release
```

Debian package

A Debian source package is provided in this repository. Build and install with:

```sh
sudo apt install debhelper-compat pkg-config libssl-dev cargo rustc
dpkg-buildpackage -b -us -uc
sudo dpkg -i ../baidunetdisk-rs_*.deb
```

Dependencies

Type Package Notes
Runtime (recommends) xdg-utils For opening files/URLs with the desktop's default handler
Runtime (suggests) fonts-noto-cjk CJK fonts for correct Chinese/Japanese/Korean text rendering
Build cargo (>= 1.88), rustc (>= 1.88) Rust toolchain
Build pkg-config, libssl-dev Native build prerequisites
Build debhelper-compat (= 13) Debian packaging

Usage

Launch the application after installation:

```sh
baidunetdisk-rs
```

On first run, sign in using either QR-code scan (scan with the Baidu Netdisk mobile app) or Cookie login (paste an existing session cookie). Once authenticated, you can browse your drive, manage files, start downloads or uploads, and preview supported document types.

Architecture

· Language: Rust (edition compatible with toolchain 1.88+)
· GUI: kasumi — pure-Rust immediate-mode GUI
· Rendering: wgpu backend
· PDF rendering: pure-Rust hayro
· Networking / crypto: libssl via OpenSSL bindings

Packaging

This project ships as a Debian package named baidunetdisk-rs:

· Section: net
· Priority: optional
· Architecture: any
· Rules-Requires-Root: no

Contributing

Issues and pull requests are welcome. Before submitting, please ensure the project builds cleanly with the minimum supported Rust version (1.88) and that any new dependencies are compatible with the pure-Rust, Linux-first design goals of the project.

License

See the upstream kasumi repository for licensing information.

Disclaimer

baidunetdisk-rs is an unofficial, third-party client and is not affiliated with, endorsed by, or supported by Baidu, Inc. "Baidu Netdisk" and "百度网盘" are trademarks of their respective owners. Use at your own risk and in accordance with Baidu's terms of service.
