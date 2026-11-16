# 开启5GA · iPhone 端

将随附 `fastboot.zip` 的 Windows 批处理流程移植到 SwiftUI + Rust，供 LiveContainer 导入。界面为中文，包含写入、配对、调试、日志四页。手机端通过现有 LocalDevVPN 连接本机，无须 Windows Python 或 iTunes。

固定载荷适用：**iPhone 14 Pro（iPhone15,2 / D73）· iOS 27.0.1 · 中国联通**。原生层在写入之前检查机型与版本；其他设备会停止。PRI 来源为原包内的 15 Pro 载荷，保留原工具中改名和修改后的键值。

## 手机使用

1. 在 LiveContainer 导入 `Fast5GA.ipa`。
2. 连接 Wi-Fi，保持 LocalDevVPN 开启、系统开发者模式开启。
3. 在“配对”页面核对 Device IP（默认 `10.7.0.1`），点击“开始本机配对”，允许本地网络访问。
4. 打开设置 → 隐私与安全性 → 开发者模式 → 与 Fast5GA 配对，在系统页面输入应用显示的 PIN。
5. 回到应用，点击“检查连接”。首次执行“完整模式”；以后每次手机重启后执行“普通模式”。

配对文件保存在应用专用目录，排除系统备份，不上传 GitHub。配对期间播放静音音频，以便切换到系统设置后维持服务。LiveContainer 宿主须允许后台音频；若宿主仍暂停子应用，可导入已有的 Remote Pairing 文件。旧式 Lockdown 配对文件不适用这个版本。

## 与原批处理对应

| 模式 | 执行顺序 |
| --- | --- |
| 完整模式 | 原版 73.0 IPCC → 检查 /var 目录 → 三件套 0644 → 其他三个 Bundle 文件 0600 → 两个 Overlay 0555 → 重载 CommCenter → 补写两个 Overlay 0555 |
| 普通模式 | 两个 Overlay 0555 → 重载 CommCenter → 补写两个 Overlay 0555 |
| 调试 1 | 仅加载原版 IPCC，并检查 /var 目录 |
| 调试 2 | 检查目录，写入三件套 0644 |
| 调试 3 | 检查目录，写入其他三个文件 0600 |
| 调试 4 | 仅写入两个 Overlay 0555 |
| 调试 5 | 仅重载 CommCenter |

重载前等待 5 秒；只向精确路径的 CommCenter 发送一次信号，最多观察 90 秒，确认同一新 PID 持续运行 15 秒后继续。重载能力会在普通/完整模式开始写入前检查。任何步骤失败均停止后续步骤。恢复原版：调试 1 后**手动重启手机**。

写入保留原批处理的“不回读目标文件”行为：通过暂存载荷已移走判断写入步骤完成，不宣称已测量目标实际权限或网络效果。ZIP 主权限和 Apple `0x5A53` 扩展权限同时设置。临时清理逐项 unlink，遇到符号链接不会沿链接递归删除；失败时保留已移出的旧文件。Books 仅保存和恢复两个配置 plist，避免重放活动数据库的 WAL/SHM。

## GitHub 编译

使用 GitHub Actions 的 macOS runner 编译，支持公开或私有仓库，无须 Apple 开发证书。输出未签名 `Fast5GA.ipa`，供 LiveContainer 导入；这不是 App Store 安装包。

仓库直接保存 `App/`、`rust-core/`、`scripts/`、`project.yml` 等可编辑源码。云端从当前提交运行：

```sh
python3 scripts/validate.py
cargo +1.100.0 test --locked --manifest-path rust-core/Cargo.toml --lib
bash scripts/build-core.sh
bash scripts/build-ipa.sh
```

在仓库的 **Releases** 中打开最新版本，分别下载 `Fast5GA.ipa` 和 `SHA256SUMS.txt`。两个文件直接作为附件发布，无需到 Actions 下载或解压产物 ZIP。构建和 CFBundle 路径检查通过后才会发布。

源码可以直接克隆到本地，在安装 Xcode、Rust 1.100.0、XcodeGen 的 Mac 上构建。`project.yml` 生成 Xcode 工程，`Cargo.lock` 固定依赖。

## 1.0.2 连接修复

1.0.1 固定拨号 49152，当前系统的 Remote Pairing 服务若改用其他端口，会出现连接被拒绝，即使其他工具和 VPN 正常。1.0.2 在每次操作前通过 `_remotepairing._tcp` 发现本机服务端口；优先通过 raw RPPairing 连接该服务，RemoteXPC 只作兼容回退；首次连接和缓存均使用选定端口。仅接受本机接口对应的服务，防止使用附近其他设备的端口；发现过程只解析 Bonjour，不打开 Remote Pairing 控制连接。

“配对”页面的服务端口留空时自动发现，发现失败先尝试同一 Device IP 上次成功的端口，无成功记录时回退到 49152，并在日志标明来源。也可填写 SideStore 网络发现或 StikDebug 显示的当前 Remote Pairing 端口。它不是配对 PIN，也不是应用配对主机临时发布的端口。端口变化不意味着配对密钥失效，不必先删除原有配对。

连接失败会保留配置 Device IP 的首个错误，并记录实际端点；不再用备用地址的最后一次超时覆盖它，也不再一律判断 VPN 未开启。

### 不连接 Wi-Fi / 飞行模式

保存配对后的检查连接和写入不再要求 Wi-Fi，且载荷均已内置，不需要互联网。首次配对仍在 Wi-Fi 下完成。先在 Wi-Fi 下完成一次连接检查，再保持 LocalDevVPN 开启，开启飞行模式并关闭 Wi-Fi，回到程序检查连接；通过后再写入。自动发现不可用时会尝试同一 Device IP 上次成功的端口，也可手动填写。检查失败则停止，不会将 VPN 接口存在视为服务可用。

此版本仅解除程序的 Wi-Fi 前置限制，并提供本机服务发现和端口回退；尚未在目标 iPhone / iOS 27.0.1 验证飞行模式下新建连接及完整写入。定位工具可保留已经建立的会话，不能仅据此推断新连接和所有服务都可用。

## 1.0.1 启动修复

载荷目录从应用根部的 `Resources` 改为 `CarrierPayloads`，避免与 CoreFoundation 的旧式 Bundle 布局目录冲突。打包后通过 Foundation/CFBundle 实际解析原名称和 LiveContainer 式改名后的 `.app`，检查可执行文件 URL 与元数据，而不只检查 ZIP 文件名是否存在。对照测试保留旧目录布局，记录它的解析结果。

## 验证范围

已完成本地 Rust 编译、载荷/原包哈希验证、菜单流程测试、ZIP 双权限测试、精确进程匹配测试。Xcode 编译结果以本仓库 Actions 为准；未在目标 iPhone / LiveContainer 内实测配对和写入，不将编译成功等同于实机验证成功。

## 来源

- 用户提供的 `fastboot.zip`（2026-11-14）：载荷、流程、权限、Grappa token 与恢复逻辑。
- [Mak5er/AirCard-iOS](https://github.com/Mak5er/AirCard-iOS)，固定上游提交 `40640c4d827aeab8d798285b57d24fcf6be070d0`：本机 RPPairing、RSD 传输与部分配对辅助代码。
- [jkcoxson/idevice](https://github.com/jkcoxson/idevice)：由上述上游随源码提供的修订版本，保留原版权声明。

上游许可见 `THIRD_PARTY/`；新增移植说明不更改原载荷的权属。
