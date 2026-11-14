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

私有仓库使用 GitHub Actions 的 macOS runner 编译，无须 Apple 开发证书。工作流仅在私有仓库执行，输出未签名 `Fast5GA.ipa`，供 LiveContainer 导入；这不是 App Store 安装包。

仓库根目录的 `Fast5GA-source.zip` 是完整源码快照。云端解包后运行：

```sh
python3 scripts/validate.py
cargo +1.100.0 test --locked --manifest-path rust-core/Cargo.toml --lib
bash scripts/build-core.sh
bash scripts/build-ipa.sh
```

在 Actions → Build private IPA → 成功的运行中，下载 `Fast5GA-LiveContainer`。工作流不自动发布 Release，构建产物保留 14 天。

源码快照也可以直接解包到本地，在安装 Xcode、Rust 1.100.0、XcodeGen 的 Mac 上构建。`project.yml` 生成 Xcode 工程，`Cargo.lock` 固定依赖。

## 验证范围

已完成本地 Rust 编译、载荷/原包哈希验证、菜单流程测试、ZIP 双权限测试、精确进程匹配测试。Xcode 编译结果以本仓库 Actions 为准；未在目标 iPhone / LiveContainer 内实测配对和写入，不将编译成功等同于实机验证成功。

## 来源

- 用户提供的 `fastboot.zip`（2026-11-14）：载荷、流程、权限、Grappa token 与恢复逻辑。
- [Mak5er/AirCard-iOS](https://github.com/Mak5er/AirCard-iOS)，固定上游提交 `40640c4d827aeab8d798285b57d24fcf6be070d0`：本机 RPPairing、RSD 传输与部分配对辅助代码。
- [jkcoxson/idevice](https://github.com/jkcoxson/idevice)：由上述上游随源码提供的修订版本，保留原版权声明。

上游许可见 `THIRD_PARTY/`；新增移植说明不更改原载荷的权属。
