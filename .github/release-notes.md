开启5GA iPhone 端 1.0.1

- 将载荷目录改为 CarrierPayloads，避免与 CoreFoundation 的旧式 Resources Bundle 布局冲突。
- 新增 Foundation/CFBundle 实际解析检查，覆盖 LiveContainer 式目录改名和 Mach-O 类型修改。
- 仓库改为完整、可编辑的源码目录。
- IPA、SHA256 校验文件作为两个独立 Release 附件发布，无需再解压产物 ZIP。

适用配置：iPhone 14 Pro（iPhone15,2）/ iOS 27.0.1 / 中国联通。

在 LiveContainer 中重新导入本版 IPA。连接 Wi-Fi、保持 LocalDevVPN 开启后，在应用中配对并检查连接；首次执行完整模式，之后每次手机重启后执行普通模式。

已通过编译、载荷校验、原生单元测试及 CFBundle 路径检查；仍需在目标 iPhone / LiveContainer 上确认实际启动、配对和写入效果。
