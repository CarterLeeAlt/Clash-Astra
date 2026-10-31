# Mihomo 版本与订阅适配契约

## 固定基线

本应用只交付 `src-tauri/mihomo.lock.json` 指定的 Mihomo `v1.19.33`、Windows x64 `amd64-v2`。契约为 `mihomo-v1.19.33-common`，上游源码提交为 `3f27ef39f8c96bbfbc8b0b9bb10f5c9ef60149d8`。权威依据是该版本的 [配置结构](https://github.com/MetaCubeX/mihomo/blob/v1.19.33/config/config.go)、[出站解析器](https://github.com/MetaCubeX/mihomo/blob/v1.19.33/adapter/parser.go)和实际二进制，不以滚动文档代替版本验收。

| 项目               | 固定值                                                             |
| ------------------ | ------------------------------------------------------------------ |
| 官方资产           | `mihomo-windows-amd64-v2-v1.19.33.zip`                             |
| 压缩包 SHA-256     | `e997417c2d50c45c314448e4749c52b912323f493f2580e475af7f9bda3dbf64` |
| 可执行文件 SHA-256 | `0b4c7353057a78de8a09db445351aadec7d72a4f48adc7d99691f3c5ff74d3da` |

不支持 Alpha、客户端内核升级或随构建查询最新版。准备、构建、测试、打包和运行前核对身份，存在的文件也必须校验；不匹配明确失败，不自动选择其他版本。重启和故障恢复使用同一个锁定内核。发布只签署应用自有程序，不重新签署内核，下载原件和交付文件哈希分别记录。

## 常用参数范围

| 类别     | 契约与验收范围                                                                                                                                               |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 节点     | SS、VMess、VLESS、Trojan、Hysteria 1/2、TUIC、AnyTLS 原生字段保真及内核加载；不代表任意远程服务器可达                                                        |
| TLS      | SNI/servername、ALPN 列表、证书校验、客户端指纹及 Reality 原生字段保留；证书 fingerprint 与 client-fingerprint 不混用                                        |
| 传输     | TCP、WS、gRPC、HTTP、H2；VLESS 原生 XHTTP，URI 中 host/path/mode 和有明确映射的 extra；复杂 URI extra 未实现时拒绝并要求原生 YAML                            |
| 协议边界 | Hysteria 1 与 2 分开；Hysteria 1 URI 转换明确拒绝，原生 YAML 保留，不能伪装成 Hysteria2                                                                      |
| 分组     | select、url-test、fallback、load-balance，成员/provider 引用交给锁定内核复核，不生成 relay 兼容分支                                                          |
| 规则     | 原始顺序、MRS domain/ipcidr、规则集合路径和实际加载；完整验收不删除生成的 DNS、规则或 provider                                                               |
| DNS      | nameserver、default-nameserver、proxy-server-nameserver、direct-nameserver、policy、respect-rules、fake-ip、IPv6；respect-rules 要求 proxy-server-nameserver |
| 订阅传输 | 短链重定向、UTF-8/BOM、响应格式、UA、流量头、更新间隔、空响应/HTML/错误版本；最大响应体 16 MiB                                                               |

原生节点未重点测试的字段仍保留并由内核处理，但不能标为已经验证。内核可能忽略未知字段，`-t` 成功不证明每个字段生效，须同时检查生成前后字段保真。

## 参数所有权

| 来源     | 负责字段和行为                                                                                                            |
| -------- | ------------------------------------------------------------------------------------------------------------------------- |
| Astra    | 本机 mixed/http/socks 端口、mode、log-level、allow-lan、全局 ipv6、external-controller、secret、进程显示策略及 TUN enable |
| 订阅     | 节点协议、分组、规则、providers、DNS 及其他非应用管理字段；DNS ipv6 不被全局开关重复改写                                  |
| 用户增强 | 显式的全局脚本、脚本链与合并项；脚本异常阻止候选提交，不把原配置回退冒充成功                                              |

TUN 打开时仅为缺失 DNS 字段补默认值，不覆盖显式 DNS。应用管理字段在增强后覆盖，不能将整个应用 Mapping 覆盖到订阅。空节点或仅内置直连不作为有效代理源。

## 分阶段验收

下载解析只证明格式和代理源结构有效，非激活订阅显示“已保存，未激活验证”。激活/更新编译最终配置、执行锁定内核 `-t`、实际加载并核对配置的代理/规则集合；集合空或未加载不标为就绪。失败通过现有事务补偿恢复前一状态。

Sublink `/clash` 在 YAML 和响应头中携带版本/契约，声明不匹配时拒绝。未声明的普通 Mihomo YAML 不被冒称为认证生成器版本，仍做格式、内核和加载检查。`profile-update-interval` 从小时转为分钟，provider `interval` 保持秒；一次性下载代理选项不改写保存偏好。明确请求的系统代理不可用时失败，不静默直连。

## 联合测试与追溯

Sublink `test:mihomo:fixtures` 使用生产 builder 导出完整无凭据配置，固化样例是 `src-tauri/tests/fixtures/sublink-mihomo.yaml`。默认后端回归使用生产下载、重定向、解析、增强入口，再由固定内核加载完整 DNS、分组、规则和九个节点。预取原 URL 公开规则集只填充原路径缓存，不修改生成器配置；资源不可达时明确失败。

真实订阅在独立目录、端口和内核中验收，`ASTRA_CONTRACT_SUBSCRIPTION_FILE` 指定私有输入，`ASTRA_CONTRACT_CONNECTIVITY=1` 额外检查真实节点连通性，不切换运行实例节点。该输入和私有日志不上传 CI。

包内 `mihomo.lock.json` 记录内核，`build-info.json` 记录应用基线、修改状态和源码指纹；本地联合交付用 `SUBLINK_SOURCE_DIR` 记录实际生成器源码身份。未提供生成器源码时该字段为空，不虚构联合验证。两个仓库的最终指纹、测试结果和线上/本地差别见验收报告。

升级必须先核对版本参数差异、修两端、通过联合回归，再更新锁定资产/哈希并交付。版本和资产地址不是可绕过的运行选项。
