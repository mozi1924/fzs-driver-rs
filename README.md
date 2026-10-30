# FZS 热敏纸打印机 Rust 驱动套件 (fzs-driver-rs)

完全开源、纯离线、尊重隐私的“夫子说 / 老师说 / 夫子机系列”（如 `FZS-C3100A`、`FZJJ`、`FZXX`）热敏打印机 Rust 重构驱动、后台守护进程与 macOS / Linux CUPS 系统打印集成。

---

## 核心特性与技术突破

1. **自包含与零系统外部 C 动态库依赖**：
   - 彻底摆脱 `brew install lzo` 或 `libcups.dylib` 动态库依赖。MiniLZO 与 CUPS/PWG 光栅解码器 100% 静态编译入 Rust 二进制。
2. **彻底修复 CUPS 打印中断与边距下溢 Bug**：
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

### 3. 一键配置 macOS / Linux 系统级 CUPS 打印机

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
