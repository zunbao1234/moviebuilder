# VideoInspector Pro

专业视频质量自动化检查工具 MVP。当前版本用于验证 Tauri 2 + React + Rust 的桌面应用主流程：MP4 导入、任务状态、真实基础信息读取、真实黑边检测、红黄绿结果展示和 HTML 报告骨架。

## 当前开发进度

- 日期：2026-05-27
- 阶段：MVP 初始化
- 状态：已完成基础工程、前端主界面、Tauri IPC、检测任务流、报告骨架、浏览器静态渲染验证，补齐 Tauri 默认图标，修复文件导入权限配置，为每个问题补齐开始/结束截图展示，接入 FFmpeg/ffprobe 真实基础信息读取、cropdetect 黑边检测、freezedetect 冻结帧检测、四角疑似 AI 标识检测和字幕 OCR/小说文本逐字匹配，新增黑边阈值与 AI 四角扫描位置设置，补强上下/左右黑边风险说明，并完成本机 FFmpeg/ffprobe 命令验证。
- 检测说明：当前版本不再固定生成 1 红 / 2 黄 / 1 绿模拟问题；已接入真实视频基础信息、真实黑边检测、真实冻结帧检测、四角疑似 AI 标识检测和字幕 OCR/小说文本逐字匹配。跳帧、音画同步、音频和编码合规性仍待后续实现。

## 技术栈

- React 18 + TypeScript
- Tailwind CSS
- Tauri 2 + Rust
- lucide-react
- npm

## 运行方式

```bash
npm install
npm run tauri dev
```

前端构建：

```bash
npm run build
```

Rust 检查：

```bash
cd src-tauri
cargo check
```

## MVP 已实现

- [x] 初始化 git 仓库和项目骨架
- [x] 配置 Vite、React、TypeScript、Tailwind CSS
- [x] 配置 Tauri 2 应用窗口和 dialog 插件
- [x] 实现文件按钮导入和文件夹 MP4 扫描
- [x] 实现窗口拖拽导入 MP4
- [x] 实现文件列表、状态、进度、红黄绿数量展示
- [x] 实现开始、暂停、取消、清空、导出报告按钮
- [x] 实现 Rust 后端检测进度事件
- [x] 实现红黄绿问题详情展示
- [x] 实现 HTML 报告骨架生成
- [x] 建立 README TODO 和开发进度记录
- [x] 完成前端 TypeScript/Vite 构建验证
- [x] 完成普通浏览器静态 UI 渲染验证
- [x] 补齐 `src-tauri/icons/icon.png`，修复 Tauri 编译期缺失图标错误
- [x] 新增 Tauri capability，授权主窗口使用 `core:default`、`core:event:default`、`dialog:default`
- [x] 放宽 Tauri 运行时检测，避免导入按钮在桌面运行时误判失效
- [x] 为每个问题生成开始/结束截图字段
- [x] 若本机或 sidecar 存在 FFmpeg，尝试抽取对应时间点真实视频帧
- [x] 截图失败时不再混入伪截图，前端和报告显示暂无截图
- [x] HTML 报告渲染问题开始/结束截图
- [x] 从 MP4 `mvhd` atom 读取基础时长，作为 ffprobe 不可用时的内部测试兜底能力
- [x] 真实问题时间段按检测工具输出生成，并对截图时间点做安全钳制
- [x] 增加 Rust 单元测试覆盖 MP4 时长读取、真实检测解析和风险阈值
- [x] 安装用户级 `ffmpeg` 命令到 `/Users/bey/.local/bin/ffmpeg`，供本机开发验证使用
- [x] 接入 `ffprobe` 读取真实视频时长、分辨率、帧率和编码信息
- [x] 接入 FFmpeg `cropdetect` 执行真实黑边检测
- [x] 黑边检测同时纳入上下和左右黑边，并在问题描述中输出对应像素和占比
- [x] 未检测到真实问题时返回 0 红 / 0 黄 / 0 绿，不再生成固定模拟结果
- [x] 黑边检测按 PRD 阈值分为红线、黄线、绿线
- [x] 后端自动查找 `src-tauri/bin`、`~/.local/bin`、`/opt/homebrew/bin`、`/usr/local/bin` 中的 `ffmpeg`/`ffprobe`
- [x] 接入 FFmpeg `freezedetect` 执行真实冻结帧检测
- [x] FFmpeg/ffprobe 缺失或检测失败时返回错误状态，不再静默当作 0 问题通过
- [x] 新增 `inspect_file(filePath, mode)` IPC 命令，便于直接验收单个文件真实检测结果
- [x] 支持在工具栏设置黑边黄线/红线阈值，并传给 Rust 后端参与风险分级
- [x] HTML 报告导出沿用当前黑边阈值设置，避免报告与界面结果不一致
- [x] 优化结果详情面板布局，红黄绿分组在宽屏下并列展示，避免绿线提示被挤到不可见区域
- [x] 新增四角疑似 AI 标识泛化检测；命中稳定高对比角标痕迹时生成红线问题
- [x] 支持设置 AI 标识四角扫描区域的边距、宽度和高度比例
- [x] 新增小说文本匹配设置区，支持粘贴小说原文并启用字幕匹配检测
- [x] 新增字幕 OCR/小说文本逐字匹配检测；字幕无法在小说文本中逐字连续匹配时生成红线问题
- [x] 字幕匹配启用但小说文本为空或 OCR 引擎缺失时返回明确错误

## 当前验证状态

- `ffmpeg -version`：通过；当前可用版本为 FFmpeg 7.1，安装方式为用户目录本地包装命令，不依赖 Homebrew 或管理员密码。
- `npm run build`：通过；本轮已从模拟结果切换到真实基础信息和黑边检测。
- 浏览器静态渲染：通过，页面可显示工具栏、文件列表空态、结果面板空态和状态栏。
- `cargo check`：通过；本轮已从模拟结果切换到真实基础信息和黑边检测。
- `cargo test`：通过；25 个 Rust 单元测试覆盖 MP4 时长读取、无证据不生成 mock 问题、cropdetect 解析、上下/左右黑边风险覆盖、黑边默认/自定义阈值、freezedetect 解析、冻结帧阈值、四角疑似 AI 标识启发式检测、AI 四角扫描区域设置、字幕 OCR 文本清洗、逐字匹配、重复 OCR 合并、字幕匹配错误处理和 base64 编码。
- `tesseract --version`：未安装；字幕 OCR 功能代码与单元测试已完成，真实端到端 OCR 需安装 Tesseract 或补齐 sidecar 后验证。
- `npm run tauri dev`：待用户本机重新运行；上一轮阻塞的默认图标、导入权限、截图不显示和模拟时间戳问题已修复。
- 桌面验证视频 `/Users/bey/Desktop/a54adf926978ecca927a6b468caf2d49 2.mp4`：ffprobe 读到 17.647s、720x1280、30fps、H.264；freezedetect 读到 6.133s-8.833s 冻结帧，持续 2.7s，应归为黄线；cropdetect 最后建议为 `crop=700:1248:10:0`。
- `npm install`：通过；npm audit 当前报告 2 个 moderate 漏洞，后续依赖升级时处理。

## 下一阶段 TODO

- [ ] 集成 FFmpeg/ffprobe sidecar，并按平台补齐二进制放置规则
- [x] 实现真实视频基础信息读取
- [x] 实现黑边检测算法和截图输出
- [x] 实现夹帧/冻结帧检测算法
- [ ] 实现跳帧/丢帧检测
- [x] 实现字幕 OCR 与小说文本逐字匹配
- [ ] 实现完整阈值设置和预设保存
- [ ] 实现 JSON 报告和批量导出
- [ ] 补充自动化测试覆盖核心状态流和报告生成
- [ ] 完成 Windows 单文件 EXE 打包验证

## 目录说明

```text
src/
  App.tsx                 # 前端主状态和 Tauri IPC 绑定
  components/             # 工具栏、文件列表、结果面板、状态栏
  types/                  # 前端共享类型
src-tauri/
  src/commands.rs         # Tauri commands 和真实检测任务
  src/report.rs           # HTML 报告生成
  src/types.rs            # Rust IPC 数据结构
  bin/                    # 后续放置 FFmpeg sidecar
```

## 开发约束

每次代码完成后都必须更新本 README，保持 TODO 进度和当前开发进度准确。
