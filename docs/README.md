# fuyun_tools 文档总览

本目录是 `fuyun_tools` 的代码级文档，目标是让开发者快速回答三个问题：

1. 项目由哪些模块组成，边界是什么？
2. 关键功能链路（剪贴板、划词、截图、录屏）怎么跑通？
3. 运行、调试、构建需要哪些前置条件和命令？

> 平台说明：当前功能主线以 Windows 为目标平台；Linux/macOS 暂未完成同等能力。

## 文档结构

- [01_Home](./wiki/01_Home.md)：项目定位、整体架构、核心数据流。
- [02_Modules](./wiki/02_Modules.md)：前端页面模块与后端 Rust 模块职责拆分。
- [03_Classes_and_Functions](./wiki/03_Classes_and_Functions.md)：关键结构体、服务与命令入口。
- [04_Dependencies](./wiki/04_Dependencies.md)：前后端依赖清单与作用说明。
- [05_Run_and_Build](./wiki/05_Run_and_Build.md)：环境准备、开发调试、构建发布。

## 建议阅读顺序

1. 先读 `01_Home` 形成全局心智模型。
2. 再读 `02_Modules` 与 `03_Classes_and_Functions`，定位代码入口。
3. 最后读 `04_Dependencies` 与 `05_Run_and_Build`，补齐依赖和工程实践细节。

## 文档维护约定

- 文档内容以仓库当前代码为准，不记录历史版本行为。
- 新增窗口、IPC 命令、核心设置项时，同步更新 `02` 与 `03`。
- 新增第三方库或脚本命令时，同步更新 `04` 与 `05`。
