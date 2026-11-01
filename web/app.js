import init, {
    generate_preview_from_rgba,
    encode_print_job_from_rgba,
    parse_printer_qr
} from './wasm/fzs_wasm.js';
import { PrinterTransport } from './transport.js';

// Global Application State
const state = {
    wasmLoaded: false,
    transport: new PrinterTransport(),
    currentSource: null, // { type: 'image' | 'pdf', data: ImageBitmap | PDFDocumentProxy, rawRgba: Uint8Array, width: number, height: number }
    pdfDoc: null,
    pdfPage: 1,
    pdfTotalPages: 1,
    config: {
        fit_mode: 'page',
        dither: 'smart_hybrid',
        threshold: 180,
        density: 180,
        paper_type: 'continuous',
        paper_spec: 'spec1',
        feed_steps: 284,
        auto_rotate: true,
        invert: false,
    },
    viewMode: 'dot_matrix', // 'dot_matrix' | 'document'
    zoom: 1.0,
    activePreviewData: null,
};

// Offscreen Canvas for pixel extraction
const offscreenCanvas = document.createElement('canvas');
const offscreenCtx = offscreenCanvas.getContext('2d', { willReadFrequently: true });

// DOM Elements
const connStatus = document.getElementById('connStatus');
const btnConnectBle = document.getElementById('btnConnectBle');
const btnConnectSerial = document.getElementById('btnConnectSerial');
const btnScanQr = document.getElementById('btnScanQr');
const dropzone = document.getElementById('dropzone');
const fileInput = document.getElementById('fileInput');
const pdfControls = document.getElementById('pdfControls');
const btnPdfPrev = document.getElementById('btnPdfPrev');
const btnPdfNext = document.getElementById('btnPdfNext');
const pdfPageLabel = document.getElementById('pdfPageLabel');
const fitModeSelect = document.getElementById('fitModeSelect');
const ditherSelect = document.getElementById('ditherSelect');
const thresholdSlider = document.getElementById('thresholdSlider');
const thresholdVal = document.getElementById('thresholdVal');
const densitySlider = document.getElementById('densitySlider');
const densityVal = document.getElementById('densityVal');
const autoRotateCheck = document.getElementById('autoRotateCheck');
const invertCheck = document.getElementById('invertCheck');
const paperLengthLabel = document.getElementById('paperLengthLabel');
const sliceCountLabel = document.getElementById('sliceCountLabel');
const btnPrint = document.getElementById('btnPrint');
const previewCanvas = document.getElementById('previewCanvas');
const previewCtx = previewCanvas.getContext('2d');
const paperSheet = document.getElementById('paperSheet');
const perforationLine = document.getElementById('perforationLine');
const btnViewDotMatrix = document.getElementById('btnViewDotMatrix');
const btnViewDocument = document.getElementById('btnViewDocument');
const btnZoomIn = document.getElementById('btnZoomIn');
const btnZoomOut = document.getElementById('btnZoomOut');
const btnZoomFit = document.getElementById('btnZoomFit');
const zoomLevel = document.getElementById('zoomLevel');
const progressModal = document.getElementById('progressModal');
const progressBar = document.getElementById('progressBar');
const progressPercent = document.getElementById('progressPercent');
const progressSlices = document.getElementById('progressSlices');
const btnCancelPrint = document.getElementById('btnCancelPrint');

// Configure PDF.js worker
if (window.pdfjsLib) {
    window.pdfjsLib.GlobalWorkerOptions.workerSrc =
        'https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.worker.min.js';
}

// 1. Initialize WebAssembly module
async function bootstrapWasm() {
    try {
        await init('./wasm/fzs_wasm_bg.wasm');
        state.wasmLoaded = true;
        console.log('✓ FZS WASM module initialized successfully');
        loadSampleTestPattern();
    } catch (err) {
        console.error('Failed to load WASM module:', err);
        alert('无法加载 WebAssembly 模块，请检查网络或浏览器支持。');
    }
}

// 2. Hardware Connection Handlers
btnConnectBle.addEventListener('click', async () => {
    try {
        const info = await state.transport.connectBle();
        updateConnectionBadge(true, info.name, 'Bluetooth');
    } catch (err) {
        console.error('BLE connection failed:', err);
        alert(`蓝牙连接失败: ${err.message}`);
    }
});

btnConnectSerial.addEventListener('click', async () => {
    try {
        const info = await state.transport.connectSerial();
        updateConnectionBadge(true, info.name, 'USB');
    } catch (err) {
        console.error('Serial connection failed:', err);
        alert(`串口连接失败: ${err.message}`);
    }
});

function updateConnectionBadge(connected, name, type) {
    if (connected) {
        connStatus.innerHTML = `
            <span class="w-2 h-2 rounded-full bg-emerald-400"></span>
            <span class="text-emerald-400 font-medium">${name} (${type})</span>
        `;
    } else {
        connStatus.innerHTML = `
            <span class="w-2 h-2 rounded-full bg-zinc-500"></span>
            <span>未连接打印机</span>
        `;
    }
}

state.transport.onStatusChange = (val) => {
    if (val === null) {
        updateConnectionBadge(false);
    }
};

// 3. QR Code Scanner / Paste
btnScanQr.addEventListener('click', () => {
    const qr = prompt('请输入或粘贴打印机背面的二维码信息 (如: C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A):');
    if (!qr) return;

    try {
        const info = parse_printer_qr(qr);
        state.config.feed_steps = info.feed_steps;
        alert(`识别成功!\n型号: ${info.model || '标准热敏机'}\nMAC: ${info.mac || 'N/A'}\n走纸步数: ${info.feed_steps} steps`);
        triggerPreviewUpdate();
    } catch (err) {
        alert('无法识别该二维码字符串: ' + err);
    }
});

// 4. File Drag & Drop and Upload
dropzone.addEventListener('click', () => fileInput.click());
fileInput.addEventListener('change', (e) => {
    if (e.target.files.length > 0) {
        handleFile(e.target.files[0]);
    }
});

dropzone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropzone.classList.add('border-blue-500', 'bg-blue-500/10');
});

dropzone.addEventListener('dragleave', () => {
    dropzone.classList.remove('border-blue-500', 'bg-blue-500/10');
});

dropzone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropzone.classList.remove('border-blue-500', 'bg-blue-500/10');
    if (e.dataTransfer.files.length > 0) {
        handleFile(e.dataTransfer.files[0]);
    }
});

async function handleFile(file) {
    if (file.type === 'application/pdf') {
        const arrayBuffer = await file.arrayBuffer();
        const pdf = await window.pdfjsLib.getDocument({ data: arrayBuffer }).promise;
        state.pdfDoc = pdf;
        state.pdfPage = 1;
        state.pdfTotalPages = pdf.numPages;
        pdfControls.classList.remove('hidden');
        await renderPdfPage(1);
    } else if (file.type.startsWith('image/')) {
        pdfControls.classList.add('hidden');
        state.pdfDoc = null;
        const bitmap = await createImageBitmap(file);
        loadBitmapToState(bitmap);
    }
}

// 5. PDF Rendering to Offscreen Canvas at 203 DPI
async function renderPdfPage(pageNum) {
    if (!state.pdfDoc) return;
    state.pdfPage = pageNum;
    pdfPageLabel.textContent = `第 ${pageNum} / ${state.pdfTotalPages} 页`;
    btnPdfPrev.disabled = pageNum <= 1;
    btnPdfNext.disabled = pageNum >= state.pdfTotalPages;

    const page = await state.pdfDoc.getPage(pageNum);
    // Target 203 DPI: 1 pt = 1/72 inch; 203 DPI scale = 203 / 72 ≈ 2.819
    const scale = 203.0 / 72.0;
    const viewport = page.getViewport({ scale });

    offscreenCanvas.width = viewport.width;
    offscreenCanvas.height = viewport.height;
    await page.render({ canvasContext: offscreenCtx, viewport }).promise;

    const imgData = offscreenCtx.getImageData(0, 0, viewport.width, viewport.height);
    state.currentSource = {
        type: 'pdf',
        rawRgba: imgData.data,
        width: viewport.width,
        height: viewport.height,
    };

    triggerPreviewUpdate();
}

btnPdfPrev.addEventListener('click', () => {
    if (state.pdfPage > 1) renderPdfPage(state.pdfPage - 1);
});
btnPdfNext.addEventListener('click', () => {
    if (state.pdfPage < state.pdfTotalPages) renderPdfPage(state.pdfPage + 1);
});

function loadBitmapToState(bitmap) {
    offscreenCanvas.width = bitmap.width;
    offscreenCanvas.height = bitmap.height;
    offscreenCtx.drawImage(bitmap, 0, 0);
    const imgData = offscreenCtx.getImageData(0, 0, bitmap.width, bitmap.height);

    state.currentSource = {
        type: 'image',
        rawRgba: imgData.data,
        width: bitmap.width,
        height: bitmap.height,
    };

    triggerPreviewUpdate();
}

// Initial sample pattern
function loadSampleTestPattern() {
    const w = 800;
    const h = 600;
    offscreenCanvas.width = w;
    offscreenCanvas.height = h;

    // Draw a nice gradient + text test pattern
    offscreenCtx.fillStyle = '#ffffff';
    offscreenCtx.fillRect(0, 0, w, h);

    offscreenCtx.fillStyle = '#000000';
    offscreenCtx.font = 'bold 36px sans-serif';
    offscreenCtx.fillText('FZS Thermal Print Studio', 40, 70);

    offscreenCtx.font = '18px monospace';
    offscreenCtx.fillText('Pure Rust WASM Driver · 1664 Dots @ 203 DPI', 40, 110);

    // Gradient bar
    const grad = offscreenCtx.createLinearGradient(40, 0, w - 40, 0);
    grad.addColorStop(0, '#000000');
    grad.addColorStop(1, '#ffffff');
    offscreenCtx.fillStyle = grad;
    offscreenCtx.fillRect(40, 140, w - 80, 50);

    // Fine lines test
    offscreenCtx.strokeStyle = '#000000';
    for (let i = 0; i < 20; i++) {
        offscreenCtx.lineWidth = (i % 3) + 1;
        offscreenCtx.beginPath();
        offscreenCtx.moveTo(40 + i * 35, 220);
        offscreenCtx.lineTo(40 + i * 35, 320);
        offscreenCtx.stroke();
    }

    const imgData = offscreenCtx.getImageData(0, 0, w, h);
    state.currentSource = {
        type: 'image',
        rawRgba: imgData.data,
        width: w,
        height: h,
    };
    triggerPreviewUpdate();
}

// 6. Reactive Preview Updates via WASM
let updateTimer = null;
function triggerPreviewUpdate() {
    clearTimeout(updateTimer);
    updateTimer = setTimeout(() => {
        executeWasmPreview();
    }, 50);
}

function executeWasmPreview() {
    if (!state.wasmLoaded || !state.currentSource) return;

    try {
        const res = generate_preview_from_rgba(
            state.currentSource.rawRgba,
            state.currentSource.width,
            state.currentSource.height,
            state.config
        );

        state.activePreviewData = res;
        paperLengthLabel.textContent = `${res.paper_length_mm.toFixed(1)} mm`;
        sliceCountLabel.textContent = `${res.slice_count.toLocaleString()} slices`;

        renderCanvasView();
    } catch (err) {
        console.error('Preview error:', err);
    }
}

function renderCanvasView() {
    if (!state.activePreviewData) return;

    const { width, height, preview_rgba } = state.activePreviewData;
    previewCanvas.width = width;
    previewCanvas.height = height;

    if (state.viewMode === 'dot_matrix') {
        const clamped = new Uint8ClampedArray(preview_rgba);
        const imgData = new ImageData(clamped, width, height);
        previewCtx.putImageData(imgData, 0, 0);
    } else {
        // Document Layout View
        previewCtx.fillStyle = '#ffffff';
        previewCtx.fillRect(0, 0, width, height);
        previewCtx.drawImage(offscreenCanvas, 0, 0, width, height);
    }

    // Apply scale and styling to paperSheet
    applyPaperScale(width, height);
}

function applyPaperScale(width, height) {
    // 1 dot @ 203 DPI ≈ 0.125 mm
    const cssWidth = Math.round(width * 0.45 * state.zoom);
    const cssHeight = Math.round(height * 0.45 * state.zoom);

    paperSheet.style.width = `${cssWidth}px`;
    paperSheet.style.height = `${cssHeight}px`;
    previewCanvas.style.width = `${cssWidth}px`;
    previewCanvas.style.height = `${cssHeight}px`;

    // Perforation line for A4 Folded
    if (state.config.paper_type === 'folded') {
        perforationLine.classList.remove('hidden');
    } else {
        perforationLine.classList.add('hidden');
    }
}

// View Mode Toggles
btnViewDotMatrix.addEventListener('click', () => {
    state.viewMode = 'dot_matrix';
    btnViewDotMatrix.className = 'px-2.5 py-1 rounded-md bg-blue-600 text-white font-medium transition';
    btnViewDocument.className = 'px-2.5 py-1 rounded-md text-zinc-400 hover:text-white transition';
    renderCanvasView();
});

btnViewDocument.addEventListener('click', () => {
    state.viewMode = 'document';
    btnViewDocument.className = 'px-2.5 py-1 rounded-md bg-blue-600 text-white font-medium transition';
    btnViewDotMatrix.className = 'px-2.5 py-1 rounded-md text-zinc-400 hover:text-white transition';
    renderCanvasView();
});

// Zoom Controls
btnZoomIn.addEventListener('click', () => {
    state.zoom = Math.min(3.0, state.zoom + 0.2);
    zoomLevel.textContent = `${Math.round(state.zoom * 100)}%`;
    renderCanvasView();
});
btnZoomOut.addEventListener('click', () => {
    state.zoom = Math.max(0.3, state.zoom - 0.2);
    zoomLevel.textContent = `${Math.round(state.zoom * 100)}%`;
    renderCanvasView();
});
btnZoomFit.addEventListener('click', () => {
    state.zoom = 1.0;
    zoomLevel.textContent = '100%';
    renderCanvasView();
});

// Parameter Control Bindings
document.querySelectorAll('.paper-type-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
        document.querySelectorAll('.paper-type-btn').forEach((b) => {
            b.className = 'paper-type-btn px-3 py-2 rounded-lg border text-xs font-medium text-left transition flex items-center gap-2 border-zinc-700 bg-zinc-800/60 text-zinc-400';
            b.querySelector('span').className = 'w-2 h-2 rounded-full bg-zinc-600';
        });

        btn.className = 'paper-type-btn active px-3 py-2 rounded-lg border text-xs font-medium text-left transition flex items-center gap-2 border-blue-500 bg-blue-500/10 text-white';
        btn.querySelector('span').className = 'w-2 h-2 rounded-full bg-blue-500';

        state.config.paper_type = btn.dataset.type;
        triggerPreviewUpdate();
    });
});

fitModeSelect.addEventListener('change', (e) => {
    state.config.fit_mode = e.target.value;
    triggerPreviewUpdate();
});

ditherSelect.addEventListener('change', (e) => {
    state.config.dither = e.target.value;
    triggerPreviewUpdate();
});

thresholdSlider.addEventListener('input', (e) => {
    state.config.threshold = parseInt(e.target.value, 10);
    thresholdVal.textContent = e.target.value;
    triggerPreviewUpdate();
});

densitySlider.addEventListener('input', (e) => {
    state.config.density = parseInt(e.target.value, 10);
    densityVal.textContent = e.target.value;
});

autoRotateCheck.addEventListener('change', (e) => {
    state.config.auto_rotate = e.target.checked;
    triggerPreviewUpdate();
});

invertCheck.addEventListener('change', (e) => {
    state.config.invert = e.target.checked;
    triggerPreviewUpdate();
});

// 7. Direct Hardware Printing
btnPrint.addEventListener('click', async () => {
    if (!state.currentSource) {
        alert('请先上传或选择要打印的图片或 PDF 文件。');
        return;
    }

    if (!state.transport.isConnected) {
        const chooseBle = confirm('当前尚未连接打印机。\n点击【确定】使用蓝牙 (BLE) 连接，点击【取消】使用 USB 串口连接。');
        try {
            if (chooseBle) {
                const info = await state.transport.connectBle();
                updateConnectionBadge(true, info.name, 'Bluetooth');
            } else {
                const info = await state.transport.connectSerial();
                updateConnectionBadge(true, info.name, 'USB');
            }
        } catch (err) {
            alert(`连接中断: ${err.message}`);
            return;
        }
    }

    try {
        // Step 1: Encode packets using pure Rust WASM
        const packets = encode_print_job_from_rgba(
            state.currentSource.rawRgba,
            state.currentSource.width,
            state.currentSource.height,
            state.config
        );

        // Step 2: Show progress modal
        progressModal.classList.remove('hidden');
        progressBar.style.width = '0%';
        progressPercent.textContent = '0%';
        progressSlices.textContent = `0 / ${packets.length} Slices`;

        // Step 3: Stream packets to hardware with flow control
        await state.transport.printJob(packets, (cur, total, pct) => {
            progressBar.style.width = `${pct}%`;
            progressPercent.textContent = `${pct}%`;
            progressSlices.textContent = `${cur} / ${total} Slices`;
        });

        setTimeout(() => {
            progressModal.classList.add('hidden');
            alert('✓ 打印指令下发完毕，正在走纸出张！');
        }, 500);
    } catch (err) {
        progressModal.classList.add('hidden');
        alert(`打印出错: ${err.message}`);
    }
});

btnCancelPrint.addEventListener('click', () => {
    state.transport.cancel();
    progressModal.classList.add('hidden');
});

// Start bootstrap
bootstrapWasm();
