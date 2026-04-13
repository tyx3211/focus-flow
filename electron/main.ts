import { app, BrowserWindow, ipcMain } from 'electron'
import { join } from 'path'
import { spawn, ChildProcess } from 'child_process'

let backendProcess: ChildProcess | null = null

function createWindow() {
    const win = new BrowserWindow({
        width: 1000,
        height: 700,
        frame: false,
        webPreferences: {
            preload: join(__dirname, 'preload.js'),
            nodeIntegration: false,
            contextIsolation: true
        }
    })

    // 生产环境隐藏菜单栏
    if (app.isPackaged) {
        win.setMenu(null)
    }

    if (process.env.VITE_DEV_SERVER_URL) {
        win.loadURL(process.env.VITE_DEV_SERVER_URL)
    } else {
        win.loadFile(join(__dirname, '../dist/index.html'))
    }

    ipcMain.on('window-minimize', () => win.minimize())
    ipcMain.on('window-maximize', () => {
        if (win.isMaximized()) win.unmaximize()
        else win.maximize()
    })
    ipcMain.on('window-close', () => win.close())
  ipcMain.on('open-user-data-folder', () => {
    const { shell } = require('electron')
    shell.openPath(app.getPath('userData'))
  })
}

app.whenReady().then(() => {
    // 只在完全打包后的生产环境下自动拉起后台服务
    if (app.isPackaged) {
        const backendPath = join(process.resourcesPath, 'backend', 'focus-flow-backend.exe')
        const userDataPath = app.getPath('userData')

        // 生成静默的子进程：windowsHide=true 保证没有黑框弹出来
        backendProcess = spawn(backendPath, [], {
            windowsHide: true,
            stdio: 'ignore',
            detached: false,
            cwd: userDataPath // 强制把 Rust 的工作目录切到 APPDATA 下，解决存取权限和被卸载清除的问题
        })

        backendProcess.on('error', (err) => {
            console.error('Failed to start native Rust backend:', err)
        })
    }

    createWindow()
})

app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
})

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') app.quit()
})

app.on('quit', () => {
    // 应用退出时自动清理掉 Rust 服务，不留僵尸进程
    if (backendProcess) {
        backendProcess.kill()
    }
})
