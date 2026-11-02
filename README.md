# FZS 热敏纸打印机 Rust 驱动套件 (fzs-driver-rs)

完全开源、纯离线、尊重隐私的“夫子说 / 老师说 / 夫子机系列”（如 `FZS-C3100A`、`FZJJ`、`FZXX`）热敏打印机 Rust 重构驱动、后台守护进程与 macOS / Linux CUPS 系统打印集成。

---

## 核心特性与技术突破

1. **100% 纯 Rust 实现与零 C 代码依赖**：
   - 彻底摒弃任何 C/C++ 依赖与系统动态库，完全使用纯 Rust 编写。
   - LZO1X-1 压缩采用纯 Rust safe 编解码器（`lzokay`），零 libc 依赖，原生支持编译为 `wasm32-unknown-unknown`。
2. **WebAssembly (WASM) & 浏览器直连驱动 (`fzs-wasm`)**：
   - 将驱动协议封装、纵向电机拉伸补偿（1:1.383）与高品质半色调抖动算法全量编译为 WASM。
   - 浏览器端利用 **Web Bluetooth API** (蓝牙) 与 **Web Serial API** (USB 串口) 直接向打印机下发切片，无需安装任何操作系统级驱动或后台守护进程。
3. **WebUI 所见即所得排版与纸张微观预览**：
   - 支持多页 PDF 与全格式图片拖拽解析（203 DPI 高精离线光栅化）。
   - 支持标准 A4 折叠纸（210mm × 297mm，带穿孔撕纸线）与连续长卷纸（毫米级高度动态计算）双模式。
   - 独家“真机热敏点阵预览 (WYSIWYG)”：无级缩放查看每一个发热点的排布，先看效果再出纸，彻底杜绝浪费热敏纸。
4. **彻底修复 CUPS 打印中断与边距下溢 Bug**：

   - 跳过系统脆弱的 `rastertopwg` 过滤器（因浮点舍入导致的 unsigned 整数下溢回绕），在 PPD 中直接声明 `*cupsFilter: "application/vnd.cups-raster 0 -"`。
   - 升级为真正的 **8-bit Grayscale** 色彩模式，避免粗糙的系统 1-bit 二值化。
3. **100% 数学与算法等价**：
   - CRC16-MODBUS（初值 `0xFFFF`，多项式 `0xA001`）逐字节一致。
   - 电机步距非对称拉伸补偿比：`MOTOR_ASPECT_RATIO = 3292.0 / 2380.0 = 1.38319`。
   - Bayer 4x4 矩阵抖动与高精 Floyd-Steinberg 误差扩散算法。
   - 208 字节每行点阵、416 字节原子切片组装。
4. **机身二维码一键绑定与机型智能推导**：
   - 直接支持机身背面二维码：`C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A`。
   - 自动识别 SN 前缀与机型：`FZJJ` 序列号自动配置 **284** 步走纸；`FZXX` 系列自动配置 **162** 步走纸。
5. **双重绑定模式与秒级快速重连**：
   - 支持二维码字串一键绑定与终端交互式蓝牙扫描绑定。
   - 持久化记录配置与 macOS CoreBluetooth UUID，后续打印**免等待扫描，直接秒连出纸**。

---

## 快速安装与使用

### 1. 编译构建
```bash
cargo build --release
# 二进制生成在 target/release/fzs-printer
```

### 2. 绑定打印机

#### 方式一：二维码一键绑定（推荐）
```bash
cargo run -p fzs-cli -- bind "C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A"
```

#### 方式二：交互式蓝牙扫描绑定
```bash
cargo run -p fzs-cli -- bind --scan
```

查看绑定详情：
```bash
cargo run -p fzs-cli -- info
```

---

### 3. 启动 WebUI 浏览器端排版预览与直连打印

无需安装 CUPS，基于 Vite、Tailwind CSS、纯 Rust WASM 与本地自包含 PDF.js 构建的离线 Web 工作台：
```bash
# 1. 首次或修改驱动后编译 WASM 与前端自包含包
./scripts/build_web.sh

# 2. 启动本地 Web 服务器并自动打开浏览器
cargo run -p fzs-cli -- web
# 自动在默认浏览器中打开 http://127.0.0.1:8080 (亦支持手机浏览器直接访问)
```
* **完全离线本地自包含**：零 CDN 外部资源依赖，Tailwind CSS、PDF.js 与 WebAssembly 驱动均打包至本地运行。
* **移动端自适应 Ergonomics**：基于现代 Web 标准构建，适配手机与平板小屏触控；提供分段视图切换（实时纸张预览 / 排版参数设置）与底部安全区便捷打印悬浮栏。
* **连接模式**：支持点击「蓝牙连接 (BLE)」通过 Web Bluetooth 无线直连（支持桌面 Chrome/Edge 及手机端兼容浏览器），或桌面「USB 串口连接」有线直连。
* **文件支持**：拖入或相册选择 PDF（支持多页选择）或 PNG/JPG/WebP 图片。
* **实时纸张预览**：支持选择连续卷纸（动态计算耗纸毫米数）或 A4 标页纸（210×297mm，带穿孔线）。
* **真机热敏点阵模拟**：实时查看由 WASM 计算生成的点阵显微级效果，动态调整二值化阈值与发热浓度。

---

### 4. 一键配置 macOS / Linux 系统级 CUPS 打印机

#### 注册系统打印队列并安装常驻守护进程：
```bash
cargo run -p fzs-cli -- cups install --launch-agent
```
安装后：
1. 打开 Mac 上的任意软件（预览 Preview、Pages、Safari、Chrome、微信等）。
2. 按 **`Cmd + P`** 打开系统打印窗口。
3. 打印机下拉选择 **`FZS-C3100A Thermal Printer`**。
4. 点击“打印”，系统即可全自动渲染并驱动热敏打印机出纸！

查看或卸载服务：
```bash
# 查看队列状态
cargo run -p fzs-cli -- cups status

# 卸载系统打印机与自启服务
cargo run -p fzs-cli -- cups uninstall --launch-agent
```

---

### 4. 命令行独立打印与测试

#### 打印自检与硬件对准测试页：
```bash
cargo run -p fzs-cli -- test
```

#### 离线打印图片或文档：
```bash
# A4 页面保持几何比例并居中留白
cargo run -p fzs-cli -- print sample.png --fit page

# 满宽连续走纸（适合长截图）
cargo run -p fzs-cli -- print receipt.png --fit width

# 指定半色调算法 (floyd-steinberg, bayer, threshold) 与加热浓度 (150-220)
cargo run -p fzs-cli -- print photo.jpg --dither floyd-steinberg --density 190
```

---

## 运行单元测试与数学等价性验证
```bash
cargo test --workspace
```
28 项跨语言数学回归测试与端到端 CUPS 光栅解码测试全部通过。

---

## 开源许可 (License)

本项目遵循 **GNU General Public License v3.0 or later (SPDX: GPL-3.0-or-later)** 开源协议。
