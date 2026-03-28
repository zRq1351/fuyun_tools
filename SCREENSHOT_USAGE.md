# 截图工具使用说明

## 快速开始

### 1. 编译项目

```bash
cd src-tauri
cargo build
```

### 2. 使用方式

#### 方式一：从JavaScript/Frontend调用

```javascript
// 调用全屏截图
import { invoke } from '@tauri-apps/api/core';

// 全屏截图并获取结果
async function takeFullScreenShot() {
    try {
        const result = await invoke('start_screenshot');
        if (result.success) {
            console.log(`截图成功: ${result.width}x${result.height}`);
            // result.png_base64 包含PNG格式的Base64数据
            // 可以直接用于显示或保存
        } else {
            console.error('截图失败:', result.error);
        }
    } catch (error) {
        console.error('调用截图命令失败:', error);
    }
}

// 区域截图
async function captureRegion(x, y, width, height) {
    try {
        const result = await invoke('capture_region', {
            x: x,
            y: y,
            width: width,
            height: height
        });
        
        if (result.success) {
            console.log(`区域截图成功: ${result.width}x${result.height}`);
            return result.png_base64;
        }
    } catch (error) {
        console.error('区域截图失败:', error);
    }
}

// 保存截图到文件
async function saveScreenshot(pngBase64) {
    try {
        const result = await invoke('save_screenshot', {
            pngBase64: pngBase64
        });
        
        if (result.success) {
            console.log('保存对话框已打开');
        }
    } catch (error) {
        console.error('保存截图失败:', error);
    }
}

// 获取屏幕尺寸
async function getScreenSize() {
    try {
        const result = await invoke('get_screen_size');
        if (result.success) {
            console.log(`屏幕尺寸: ${result.width}x${result.height}`);
            return { width: result.width, height: result.height };
        }
    } catch (error) {
        console.error('获取屏幕尺寸失败:', error);
    }
}

// 打开截图编辑窗口
async function openScreenshotEditor() {
    try {
        await invoke('open_screenshot_editor');
    } catch (error) {
        console.error('打开截图编辑器失败:', error);
    }
}
```

#### 方式二：从Rust后端调用

```rust
use crate::features::screenshot::capture;

// 全屏截图
fn take_full_screen_shot() -> Result<(), String> {
    let (rgba_data, width, height) = capture::capture_full_screen()?;
    
    // 转换为Base64
    let png_base64 = capture::rgba_to_base64_png(&rgba_data, width, height)?;
    
    println!("截图成功: {}x{}", width, height);
    // 可以保存到文件或进行其他处理
    
    Ok(())
}

// 区域截图
fn capture_screen_region(x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
    let (rgba_data, w, h) = capture::capture_screen_region(x, y, width, height)?;
    
    println!("区域截图成功: {}x{}", w, h);
    
    Ok(())
}

// 获取屏幕尺寸
fn get_screen_dimensions() -> Result<(u32, u32), String> {
    capture::get_screen_size()
}
```

## 完整示例：创建截图按钮

```vue
<template>
  <div>
    <button @click="takeFullScreen">全屏截图</button>
    <button @click="takeRegionShot">区域截图</button>
    <button @click="openEditor">打开编辑器</button>
    
    <div v-if="screenshotImage">
      <img :src="screenshotImage" alt="截图预览" />
      <button @click="saveToClipboard">复制到剪贴板</button>
      <button @click="saveToFile">保存到文件</button>
    </div>
  </div>
</template>

<script>
import { invoke } from '@tauri-apps/api/core';

export default {
  data() {
    return {
      screenshotImage: null
    }
  },
  methods: {
    async takeFullScreen() {
      try {
        const result = await invoke('start_screenshot');
        if (result.success) {
          this.screenshotImage = `data:image/png;base64,${result.png_base64}`;
        }
      } catch (error) {
        console.error('截图失败:', error);
      }
    },
    
    async takeRegionShot() {
      // 区域截图需要先选择区域
      // 这里示例：截图屏幕中间的800x600区域
      try {
        const screenSize = await invoke('get_screen_size');
        if (screenSize.success) {
          const x = Math.floor((screenSize.width - 800) / 2);
          const y = Math.floor((screenSize.height - 600) / 2);
          
          const result = await invoke('capture_region', {
            x: x,
            y: y,
            width: 800,
            height: 600
          });
          
          if (result.success) {
            this.screenshotImage = `data:image/png;base64,${result.png_base64}`;
          }
        }
      } catch (error) {
        console.error('区域截图失败:', error);
      }
    },
    
    async openEditor() {
      try {
        await invoke('open_screenshot_editor');
      } catch (error) {
        console.error('打开编辑器失败:', error);
      }
    },
    
    async saveToClipboard() {
      // 复制Base64数据到剪贴板
      if (this.screenshotImage) {
        const base64Data = this.screenshotImage.split(',')[1];
        try {
          await navigator.clipboard.writeText(base64Data);
          alert('已复制到剪贴板');
        } catch (error) {
          console.error('复制失败:', error);
        }
      }
    },
    
    async saveToFile() {
      if (this.screenshotImage) {
        const base64Data = this.screenshotImage.split(',')[1];
        try {
          await invoke('save_screenshot', { pngBase64: base64Data });
        } catch (error) {
          console.error('保存失败:', error);
        }
      }
    }
  }
}
</script>
```

## 快捷键支持

### 全局快捷键（可在设置中配置）

- `Ctrl + Shift + Z` - 打开文字剪贴板窗口
- `Ctrl + Shift + X` - 打开图片剪贴板窗口
- `Ctrl + Shift + S` - 快速截图（默认）

### 截图编辑器快捷键

- `Ctrl + C` - 复制到剪贴板
- `Ctrl + S` - 保存到文件
- `Ctrl + Z` - 撤销
- `Ctrl + Y` - 重做
- `Esc` - 取消/关闭

## 注意事项

1. **权限要求**：截图功能需要屏幕捕获权限
2. **平台支持**：支持Windows、macOS、Linux
3. **图片格式**：输出为PNG格式，Base64编码
4. **性能考虑**：大分辨率截图可能占用较多内存

## 故障排除

### 截图失败

- 检查屏幕捕获权限
- 确认screenshots依赖已正确安装
- 查看日志输出获取详细错误信息

### 编译错误

- 确保Cargo.toml中包含必要的依赖
- 运行 `cargo clean && cargo build` 重新编译

### 前端调用失败

- 确认Tauri IPC配置正确
- 检查命令名称拼写
- 查看浏览器控制台错误信息