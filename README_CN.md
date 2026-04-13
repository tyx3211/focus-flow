# Focus Flow

Focus Flow 是一个用于监控 API 账号额度使用情况的 Windows 桌面小工具。项目由 Vue 3 + Vite 渲染层、Electron 桌面外壳和 Rust 本地后端服务组成。

## 功能概览

- 支持添加多个 API 会话，凭据格式可以是 JWT、`sess-` 字符串或 `auth.json` 内容。
- 通过本地 Rust HTTP 服务轮询额度和账单接口。
- 在桌面界面中展示连接状态、使用量、重置时间窗口和手动刷新操作。
- 本地账号和设置数据不进入 git 仓库。

## 项目结构

```text
.
├── backend/          # Rust 后端服务，监听 127.0.0.1:48123
├── electron/         # Electron 主进程和 preload
├── src/              # Vue 渲染层界面
├── index.html        # Vite 入口
├── package.json      # npm 脚本和 Electron Builder 配置
└── vite.config.ts    # Vite + Electron 插件配置
```

## 环境要求

- Node.js 20 或更新版本
- npm
- Rust stable 工具链
- Windows，用于打包 `.exe` 安装包

## 开发运行

安装前端依赖：

```powershell
npm install
```

在一个终端中启动 Rust 后端：

```powershell
cd backend
cargo run
```

在另一个终端中启动 Electron/Vite 开发应用：

```powershell
npm run dev
```

开发模式下，Electron 主进程不会自动拉起后端服务。前端界面默认访问 `http://127.0.0.1:48123`。

## 打包构建

在项目根目录构建桌面应用：

```powershell
npm run build
```

`build` 脚本会先用 release 模式编译 Rust 后端，然后再执行 Vue/Vite 构建和 Electron Builder 打包。

打包后的 Electron 应用会从 `backend/target/release/focus-flow-backend.exe` 复制后端程序，这个路径由 `package.json` 中的 `extraResources` 配置指定。

## 本地数据

后端会读写以下文件：

- `accounts.json`
- `settings.json`

这些文件可能包含账号凭据或个人设置，因此已被 `.gitignore` 排除。依赖目录、前端和 Electron 构建产物、Rust `target` 目录、安装包目录、本地工具二进制等也不会进入 git 仓库。

## 常用脚本

```powershell
npm run dev      # 启动 Vite/Electron 开发模式
npm run build:backend # 构建 Rust 后端 release 二进制
npm run build    # 构建后端、类型检查、构建渲染层和 Electron 输出，并使用 electron-builder 打包
npm run preview  # 预览 Vite 构建结果
```
