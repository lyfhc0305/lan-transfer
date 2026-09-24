# 邻传 · LAN Transfer

Mac 菜单栏 / Windows 系统托盘中的局域网文件互传工具。

当前版本 **0.3.0**。基于本项目轻量 Rust 核心开发，LocalSend 官方源码保留在 `localsend/` 作为参考；没有修改、重命名或重新打包 LocalSend。

## 0.3 的变化

- **不再需要连接密钥。** 每台电脑首次运行时生成自己的设备密钥，打开就能在“附近的设备”里看到同一网络的其他电脑，设备自动出现和消失，不需要刷新。
- **整批只确认一次。** 一次发送的所有文件、文件夹作为一个请求，确认框列出全部内容和总大小。
- **信任设备。** 确认时勾选“信任此设备”，以后这台设备发来的文件直接保存；可在设置中移除，也可开启“只接收已信任设备”。信任只在确认框中进行，此时对方身份已由握手证明。
- **发送文件夹和文字。** 文件夹保留目录结构（跳过符号链接和 .DS_Store 等系统文件）；文字和链接与文件一样先经接收方确认，对方可一键复制或打开。
- **接收方失败原因回传。** 磁盘空间不足、校验失败等会作为原因显示在发送方的记录里；接收前检查磁盘空间，放不下时确认框提醒、已信任设备的请求直接拒绝。
- **系统通知。** 窗口收起时收到文件、传输失败和第一次收起都会发系统通知（macOS 通知中心 / Windows 通知）。
- **界面重新设计。** 主页＝附近的设备＋发送内容＋发送按钮；传输记录与设置为子页面。传输进度显示在主页底部，失败可重试，收到的文件可直接打开或在访达 / 资源管理器中显示。设置即时生效。浅色 / 深色跟随系统。
- Mac 上 ⌘Q 在有传输时会先询问（关闭了 winit 的默认菜单）。

**0.3 与 0.2 协议不兼容**，两台电脑都需要更新。旧设置中的名称、保存位置、接收与托盘选项会保留，旧的连接密钥和“自动接收”不再使用。

详见 [改版说明](docs/改版说明-2026-09-24.md)，使用方法见 [使用说明](packaging/使用说明.txt)。

## 协议

- **发现**：UDP 45873。每 3 秒向各网卡的广播地址、255.255.255.255、已信任设备的上次地址和手动输入的地址发送 `hello`，收到的一方以 `reply` 回应；10 秒未响应的设备移出列表，退出或关闭接收时发送 `bye`。
- **传输**：TCP 45873，一个连接传一批。握手为 Noise XX（X25519 / ChaCha20-Poly1305 / SHA-256），双方证明各自的设备密钥（公钥即设备 ID），每个连接使用新的会话密钥。之后：发送方 `Offer`（全部条目或文字）→ 接收方 `Reply`（未信任时先询问用户）→ 按顺序逐个文件发送数据帧和 SHA-256 → 接收方 `Receipt`。
- 接收方先完整校验条目（路径穿越、Windows 保留名与非法字符、方向控制字符、大小写重复、文件与文件夹冲突、数量与大小上限）再写盘；文件先写临时文件，校验通过后以不覆盖方式落盘，顶层重名时编号。
- 首次连接为“首次信任”模型：未信任设备的请求（文件或文字）需要用户确认，确认框显示对方指纹，可与对方设置页的本机指纹核对。设备列表来自未认证的广播，所以列表本身不提供“信任”入口。
- 接收方在接受之后失败时（磁盘满、校验失败）发送带原因的 `Receipt` 并读完对方已发出的数据再关闭；发送方一发现对方提前回话就停止发送并读取原因。

## 工程结构

- `src/main.rs`：入口、单实例、窗口与菜单栏 / 托盘启动。
- `src/model.rs`：设置、设备身份、共享状态与格式化。
- `src/wire.rs`：Noise 握手与加密分帧。
- `src/network.rs`：批量发送与接收、条目扫描与校验。
- `src/discovery.rs`：设备发现。
- `src/notify.rs`：系统通知（notify-rust）。
- `src/ui/`：`mod.rs`（窗口框架、页眉、提示条、拖放）、`home.rs`（主页）、`transfers.rs`（传输记录）、`settings.rs`（设置）、`dialogs.rs`（收件确认、收到文字、退出确认、输入 IP）、`widgets.rs`、`theme.rs`、`icons.rs`、`tray.rs`、`demo.rs`（截图用演示数据）。
- `assets/`：界面字体、应用图标及字体许可。`NotoSansSC.ttf` 是可变字体源文件，仅供 `make_fonts.py` 生成两个静态字体，不打包进程序。
- `packaging/`：Windows 资源、清单及使用说明。
- `scripts/`：构建打包、字体与图标生成。
- `prototypes/`：早期命令行原型及其测试脚本存档。
- `localsend/`：独立克隆的上游参考仓库。

## 构建与打包

在 Mac 上运行：

```
python3 scripts/build_release.py
```

生成 `dist/` 下的 Mac 通用版（zip 与 dmg）和 Windows x64 便携包。需要 Rust 1.85 或更新（`rustup update`）、Apple Command Line Tools，以及项目目录 `.build-tools/` 中的 LLVM-MinGW 工具链。脚本会添加所需的 Rust 目标，不安装或启动应用，也不执行测试。

字体与图标已生成在 `assets/` 中，通常无需重新生成：

- `python3 scripts/make_fonts.py`：从可变字体生成 `NotoSansSC-Regular.ttf` 与半粗子集 `NotoSansSC-SemiBold.ttf`，需要 `python3 -m pip install fonttools`。界面新增 GB 2312 以外的汉字时，测试 `heading_font_covers_every_ui_character` 会提示重新生成。
- `python3 scripts/make_icons.py`：生成 `AppIcon.icns` 与 `AppIcon.ico`。

## 测试

`cargo test --release -- --test-threads=1`，共 22 项，说明见 `tests/README.md`。Windows 目标可在 Linux 上做类型检查：安装 `binutils-mingw-w64-x86-64`，以 `WINDRES=<带 --preprocessor=/usr/bin/cpp 的 windres 包装脚本>` 运行 `cargo check --target x86_64-pc-windows-gnullvm`。

Linux 仅用于开发和测试：`Cargo.toml` 为 Linux 打开 eframe 的 X11 支持，Linux 版不显示托盘。截图用演示数据：`cargo build --release --features demo` 后以 `LAN_TRANSFER_DEMO=<场景>`（home、busy、text、transfers、settings、request、request-text、request-space、message、address、exit、toast、empty）和 `LAN_TRANSFER_THEME=dark` 启动；演示模式不收发任何数据。`LAN_TRANSFER_CONFIG_DIR` 可指定独立的设置目录。

## 参考来源

- [LocalSend](https://github.com/localsend/localsend)
- [Noise Protocol Framework](https://noiseprotocol.org/) · [snow](https://github.com/mcginty/snow)
- [egui](https://github.com/emilk/egui)
- [tray-icon](https://github.com/tauri-apps/tray-icon)
