# 后端测试默认执行与 CI 环境验证记录

适用范围：2026-10-31，基于提交 `b41528f6301e704a6ce2e67ad0794eb77c28a563` 的工作树修改。本文记录本次改造和实测；当前操作要求见 [构建、检查与打包](../release/build-and-package.md)。

## 改动范围

- 两项环境测试移除 `#[ignore]`，普通 `cargo test` 默认执行；环境失败必须报错，不跳过、不降级为通过。
- 四站探测默认使用本机 `7897`；测试入口可显式传入 `ASTRA_TEST_MIXED_PORT`，四站全部记录后统一断言，不因首站失败遗漏后续站点。
- 独立内核集成默认使用仓库内无凭据的 `src-tauri/tests/fixtures/mihomo-control.yaml`。`ASTRA_TEST_SUBSCRIPTION_FILE` 仍可显式指定其他工作区测试输入，指定文件不可用直接失败；不要求真实订阅。
- `test:backend` 入口启动无订阅 Mihomo，校验配置、等待控制器就绪，然后运行全部后端测试；核对零失败、零忽略、零过滤及两项测试存在。默认 `MATCH,DIRECT`；显式 `--upstream-proxy` 可指定无认证信息的 HTTP 上游，不能在失败后自动切换出口。
- CI 使用同一 `test:backend` 入口，不传本机代理参数。无论测试成败归档日志和汇总 JSON，保留 7 天，不上传配置；保留前端、Windows service 和其他现有检查及发布构建。
- 入口设置单轮 20 分钟预算、就绪上限 15 秒，每分钟输出进度；只清理该入口创建的命令树，不按进程名终止用户程序。CI 另设 25 分钟步骤预算。

## 本机实测

使用已有工作区 Node.js `24.18.0`、pnpm `11.15.1`、Rust `1.99.1`。Rust 离线运行，临时目录和产物均在工作区内；本轮未设置 `ASTRA_TEST_SUBSCRIPTION_FILE`，验证默认嵌入夹具可以执行。

命令均通过工作区 Node 与 `scripts/pnpm.mjs` 执行：

| 场景                       | 命令或参数                                                                                      | 结果                                                                                           |
| -------------------------- | ----------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| 本机当前代理、默认完整测试 | `exec cargo test --manifest-path src-tauri/Cargo.toml --locked -- --nocapture --test-threads=1` | 76 通过、0 失败、0 忽略；独立内核集成在同一轮执行                                              |
| CI 式无订阅直连环境        | `run test:backend --mixed-port 0`                                                               | 75 通过、1 失败、0 忽略；Google、YouTube、ChatGPT 超时，GitHub 436 ms；明确返回非零            |
| 无订阅环境、显式本机上游   | `run test:backend --mixed-port 0 --upstream-proxy http://127.0.0.1:20081`                       | 76 通过、0 失败、0 忽略、0 过滤；全过程约 30 秒，两个真实 Mihomo 实例均清理                    |
| 端口已占用                 | 测试程序持有独立端口，再指定该端口启动入口                                                      | `EADDRINUSE`、退出码 1；未启动任何测试子进程、不终止原监听者                                   |
| 参数拒绝                   | 非法端口、未知跳过参数、带认证信息的上游 URL                                                    | 均返回 1，不启动测试，不接受跳过或过滤开关                                                     |
| 工作流                     | 使用工作区已有 YAML 解析器解析，并核对步骤顺序和参数                                            | 通过；资源准备先于测试、测试先于发布构建、CI 未传上游，日志归档仅允许 `.log` 和 `summary.json` |

最后复核的 TypeScript 类型、前端 7 项测试、Rust 格式与编译、架构、命名、工作区隔离、图标、Prettier 和差异空白检查均通过，命令退出码见 `.tmp/backend-ci-verification/checks.json`。脚本语法检查也通过；工作流仅完成本机 YAML 解析和结构核对，不冒充 GitHub 服务端校验或远程执行结果。

| 站点    | 本机已有代理 | 独立测试 Mihomo 经显式上游 |
| ------- | ------------ | -------------------------- |
| Google  | 559 ms       | 571 ms                     |
| YouTube | 557 ms       | 550 ms                     |
| ChatGPT | 554 ms       | 539 ms                     |
| GitHub  | 568 ms       | 600 ms                     |

四站测试通过真实代理发送 CONNECT 请求和 TLS ClientHello，收到首个响应字节后结束；不完成 TLS 握手、不校验证书、不发完整网页请求。上述数值不是网页加载时间或业务可用性证明。

独立真实内核集成覆盖配置编译及校验、控制器就绪、代理表读取、模式修改、错误鉴权拒绝、无效配置回滚、内核接受后提交失败回滚和正常提交。夹具含一个本机 SOCKS 节点，仅验证控制接口，不验证远程节点质量。

## 证据与收尾

- 本机已有代理测试：`.tmp/default-required-tests-20261031/backend-tests.log`、`test-results.json` 和 `final-state.json`。
- 直连失败：`.tmp/backend-tests/d9f09bdd-66ca-4bc5-b645-cd3b1e64e93a/` 中的日志和 `summary.json`。
- 显式上游成功：`.tmp/backend-tests/73fd4859-a19e-4b88-b473-dc652d31751d/` 中的日志和 `summary.json`。
- YAML、步骤顺序、参数拒绝、端口占用及清理断言：`.tmp/backend-ci-verification/infrastructure-checks.json`。
- 最终运行实例、进程残留和便携包核对：`.tmp/backend-ci-verification/final-state.json`。

最初一次临时驱动按 JSON 键顺序比较前后进程快照，产生辅助收尾误报；结构化字段核对通过，本机已有代理 PID 始终为 `52100`，独立测试内核均退出。没有更改原始测试结果或通过重试掩盖断言失败；直连失败记录完整保留。

临时验证驱动、运行配置和缓存移入回收站；保留上表实际引用的测试日志与校验 JSON。新便携 ZIP 的 SHA-256 仍为 `a7014b677695661ad07671ef67330ffe1b2b06ba26eb140d0c73beb66b7c6005`。

## 验证边界

- 本机与 CI 使用相同测试用例及断言，但出口不同；本机显式上游通过不能代表 GitHub runner 的 DIRECT 出口已经验证。远程 CI 本轮未触发，外网波动会照常导致 CI 失败。
- 没有重新下载真实订阅，也没有运行远程节点吞吐、完整网页、TUN、服务安装或防火墙变更。
- 未提交、推送、发布或重新打包；业务代码和用户配置未修改，原便携包保持不变。
