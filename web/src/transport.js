/**
 * Browser Hardware Transport for FZS / TeacherSaid Thermal Printers
 * Supports Web Bluetooth (BLE) and Web Serial (USB cable).
 */

export const SERVICE_UUIDS = [
    // FZS-C3100A & TeacherSaid primary service (0x00FF)
    '000000ff-0000-1000-8000-00805f9b34fb',
    // FZS-C3100A advertised service (0x00EE)
    '000000ee-0000-1000-8000-00805f9b34fb',
    // Standard thermal printer vendor UART service (0xFF00)
    '0000ff00-0000-1000-8000-00805f9b34fb',
    // Generic thermal printer GATT service (0xFFF0)
    '0000fff0-0000-1000-8000-00805f9b34fb',
    // HM-10 / CC254x serial service (0xFFE0)
    '0000ffe0-0000-1000-8000-00805f9b34fb',
    // Tencent AirSync profile (0xFEE7)
    '0000fee7-0000-1000-8000-00805f9b34fb',
    // JieLi AC69 / JL BLE UART profiles
    '0000ae00-0000-1000-8000-00805f9b34fb',
    '0000ae30-0000-1000-8000-00805f9b34fb',
    '0000af30-0000-1000-8000-00805f9b34fb',
    // Additional printer vendor services
    '0000fe59-0000-1000-8000-00805f9b34fb',
    '0000fe00-0000-1000-8000-00805f9b34fb',
    '000018f0-0000-1000-8000-00805f9b34fb',
    // Microchip / ISSC Transparent UART
    '49535343-fe7d-4ae5-8fa9-9fafd205e455',
    // Nordic UART Service (NUS)
    '6e400001-b5a3-f393-e0a9-e50e24dcca9e',
    // PeriPage / Paperang
    'e7810a71-73ae-499d-8c15-faa9aef0c3f2',
];

export const WRITE_CHAR_UUIDS = [
    '0000ff01-0000-1000-8000-00805f9b34fb', // FZS-C3100A Write & Notify Char
    '0000fff1-0000-1000-8000-00805f9b34fb',
    '0000ffe1-0000-1000-8000-00805f9b34fb',
    '0000fff2-0000-1000-8000-00805f9b34fb',
    '0000ff02-0000-1000-8000-00805f9b34fb',
    '0000ae01-0000-1000-8000-00805f9b34fb',
    '0000ae02-0000-1000-8000-00805f9b34fb',
    '49535343-8841-43f4-a8d4-ecbe34729bb3',
    '6e400002-b5a3-f393-e0a9-e50e24dcca9e',
    'bef8d6c9-9c21-4c9e-b632-bd58c1009f9f',
];

export const NOTIFY_CHAR_UUIDS = [
    '0000ff01-0000-1000-8000-00805f9b34fb', // FZS-C3100A also has NOTIFY on ff01
    '0000ff02-0000-1000-8000-00805f9b34fb',
    '0000fff2-0000-1000-8000-00805f9b34fb',
    '0000ffe2-0000-1000-8000-00805f9b34fb',
    '0000fff1-0000-1000-8000-00805f9b34fb',
    '0000ffe1-0000-1000-8000-00805f9b34fb',
    '0000ae02-0000-1000-8000-00805f9b34fb',
    '49535343-1e4d-4bd9-ba61-23c647249616',
    '6e400003-b5a3-f393-e0a9-e50e24dcca9e',
];

export class PrinterTransport {
    constructor() {
        this.type = null; // 'ble' | 'serial'
        this.device = null;
        this.server = null;
        this.writeChar = null;
        this.notifyChar = null;
        this.serialPort = null;
        this.serialWriter = null;
        this.isConnected = false;
        this.isPrinting = false;
        this.cancelRequested = false;
        this.bufferPaused = false;
        this.onStatusChange = null;
    }

    async connectBle() {
        if (!navigator.bluetooth) {
            throw new Error('当前浏览器不支持 Web Bluetooth API，请使用桌面端 Chrome、Edge 或 Android Chrome 访问。');
        }

        // If previously connected, disconnect cleanly first
        if (this.isConnected) {
            await this.disconnect();
        }

        let device;
        try {
            device = await navigator.bluetooth.requestDevice({
                filters: [
                    { namePrefix: 'FZS' },
                    { namePrefix: 'FZ' },
                    { namePrefix: 'fzs' },
                    { namePrefix: 'fz' },
                    { namePrefix: 'TP' },
                    { namePrefix: 'Printer' },
                    { namePrefix: 'printer' },
                    { namePrefix: 'Teacher' },
                    { namePrefix: 'teacher' },
                    { namePrefix: 'POS' },
                    { namePrefix: 'pos' },
                    { namePrefix: 'BLE' },
                    { namePrefix: 'C3100' },
                    { services: [0x00FF] },
                    { services: [0x00EE] },
                    { services: [0xFFF0] },
                    { services: [0xFFE0] },
                    { services: [0xFF00] },
                ],
                optionalServices: SERVICE_UUIDS,
            });
        } catch (err) {
            // User cancelled device picker
            throw err;
        }

        this.device = device;
        device.addEventListener('gattserverdisconnected', () => this.handleDisconnect());

        let server = null;
        try {
            console.log(`Connecting to BLE peripheral "${device.name}" (${device.id})...`);
            server = await device.gatt.connect();
            this.server = server;

            // 1. Gather all accessible primary services
            const discoveredServices = [];
            try {
                const svcs = await server.getPrimaryServices();
                if (svcs && svcs.length > 0) {
                    discoveredServices.push(...svcs);
                }
            } catch (e) {
                console.warn('getPrimaryServices() batch lookup failed, probing individual UUIDs:', e);
            }

            if (discoveredServices.length === 0) {
                for (const uuid of SERVICE_UUIDS) {
                    try {
                        const s = await server.getPrimaryService(uuid);
                        if (s) discoveredServices.push(s);
                    } catch (_) {}
                }
            }

            if (discoveredServices.length === 0) {
                throw new Error('未找到兼容的热敏打印机 GATT 服务 (0x00FF/0xFFF0/0xFFE0 等)。请确保打印机已开机并在附近。');
            }

            console.log(`Discovered ${discoveredServices.length} matching GATT services on printer:`, discoveredServices.map(s => s.uuid));

            // 2. Discover all characteristics across discovered services
            const allCharacteristics = [];
            for (const svc of discoveredServices) {
                try {
                    const chars = await svc.getCharacteristics();
                    allCharacteristics.push(...chars);
                } catch (e) {
                    console.warn(`Failed to inspect characteristics on service ${svc.uuid}:`, e);
                }
            }

            console.log(`Discovered ${allCharacteristics.length} characteristics on printer:`, allCharacteristics.map(c => c.uuid));

            // 3. Match Write Characteristic
            // Priority: Known write UUID > Any characteristic with write / writeWithoutResponse
            let writeChar = null;
            for (const known of WRITE_CHAR_UUIDS) {
                const found = allCharacteristics.find(
                    (c) => c.uuid.toLowerCase() === known.toLowerCase()
                );
                if (found && (found.properties.write || found.properties.writeWithoutResponse)) {
                    writeChar = found;
                    break;
                }
            }
            if (!writeChar) {
                writeChar = allCharacteristics.find(
                    (c) => c.properties.write || c.properties.writeWithoutResponse
                );
            }

            if (!writeChar) {
                throw new Error('未在打印机上找到可写入的 GATT 特征值 (Write Characteristic)。');
            }
            this.writeChar = writeChar;

            // 4. Match Notify Characteristic
            // Priority: Known notify UUID > Any characteristic with notify / indicate
            let notifyChar = null;
            for (const known of NOTIFY_CHAR_UUIDS) {
                const found = allCharacteristics.find(
                    (c) => c.uuid.toLowerCase() === known.toLowerCase()
                );
                if (found && (found.properties.notify || found.properties.indicate)) {
                    notifyChar = found;
                    break;
                }
            }
            if (!notifyChar) {
                notifyChar = allCharacteristics.find(
                    (c) => c.properties.notify || c.properties.indicate
                );
            }

            if (notifyChar) {
                try {
                    await notifyChar.startNotifications();
                    notifyChar.addEventListener('characteristicvaluechanged', (e) => this.handleNotify(e));
                    this.notifyChar = notifyChar;
                    console.log('✓ BLE Notifications subscribed successfully on:', notifyChar.uuid);
                } catch (err) {
                    console.warn('启动状态通知监听失败 (非致命):', err);
                }
            }

            this.type = 'ble';
            this.isConnected = true;
            console.log(`✓ BLE Connected! Write Char: ${writeChar.uuid}, Notify Char: ${notifyChar?.uuid || 'None'}`);

            return {
                name: device.name || 'FZS Thermal Printer',
                type: 'Bluetooth BLE',
            };
        } catch (err) {
            // CRITICAL FIX FOR DEADLOCK:
            // Always immediately disconnect GATT if connection setup or discovery fails.
            // Leaving the BLE connection half-open holds the printer's BLE radio in CONNECTED state,
            // preventing advertising and blocking all CLI/web reconnections until hardware power cycle!
            console.error('BLE connection or GATT discovery failed; aborting and cleanly disconnecting:', err);
            if (server && server.connected) {
                try { server.disconnect(); } catch (_) {}
            } else if (device.gatt && device.gatt.connected) {
                try { device.gatt.disconnect(); } catch (_) {}
            }
            this.handleDisconnect();
            throw err;
        }
    }

    async connectSerial() {
        if (!navigator.serial) {
            throw new Error('当前浏览器不支持 Web Serial API，请使用桌面端 Chrome 或 Edge 访问。');
        }

        if (this.isConnected) {
            await this.disconnect();
        }

        const port = await navigator.serial.requestPort();
        await port.open({ baudRate: 115200 });

        this.serialPort = port;
        this.serialWriter = port.writable.getWriter();
        this.type = 'serial';
        this.isConnected = true;

        return {
            name: 'USB Serial Thermal Printer',
            type: 'USB Serial (115200)',
        };
    }

    handleNotify(event) {
        const val = new Uint8Array(event.target.value.buffer);
        // FA 5A FB indicates printer internal queue buffer full
        if (val.length >= 3 && val[0] === 0xFA && val[1] === 0x5A && val[2] === 0xFB) {
            this.bufferPaused = true;
            console.warn('⚠️ 打印机内部硬件缓冲区已满 (FA 5A FB)，流控暂停 5 秒...');
            setTimeout(() => {
                this.bufferPaused = false;
                console.info('✓ 打印机缓冲区恢复就绪，继续下发数据。');
            }, 5000);
        }
        if (this.onStatusChange) {
            this.onStatusChange(val);
        }
    }

    handleDisconnect() {
        this.isConnected = false;
        this.isPrinting = false;
        this.bufferPaused = false;
        this.writeChar = null;
        this.notifyChar = null;
        this.server = null;
        this.device = null;
        if (this.onStatusChange) {
            this.onStatusChange(null);
        }
    }

    async writePacket(packet, delayMs = 12) {
        if (!this.isConnected) throw new Error('打印机连接已断开');

        // Wait if printer signaled buffer full
        while (this.bufferPaused) {
            await new Promise((r) => setTimeout(r, 100));
        }

        if (this.type === 'ble') {
            const hasWriteWithResp = this.writeChar.properties.write;
            const writeFn = (hasWriteWithResp && this.writeChar.writeValueWithResponse)
                ? (p) => this.writeChar.writeValueWithResponse(p)
                : (this.writeChar.writeValueWithoutResponse
                    ? (p) => this.writeChar.writeValueWithoutResponse(p)
                    : (p) => this.writeChar.writeValue(p));

            try {
                await writeFn(packet);
            } catch (err) {
                // If GATT was busy or rejected, wait a moment and retry once
                console.warn('GATT 写入遇到临时繁忙，50ms 后重试一次...', err);
                await new Promise((r) => setTimeout(r, 50));
                await writeFn(packet);
            }
        } else if (this.type === 'serial') {
            await this.serialWriter.write(packet);
        }

        if (delayMs > 0) {
            await new Promise((r) => setTimeout(r, delayMs));
        }
    }

    async printJob(packets, onProgress) {
        if (!this.isConnected) throw new Error('尚未连接到任何打印机。');
        this.isPrinting = true;
        this.cancelRequested = false;

        const total = packets.length;
        try {
            for (let i = 0; i < total; i++) {
                if (this.cancelRequested) {
                    throw new Error('用户已取消打印任务。');
                }

                const pkt = packets[i];
                // Handshake and setup packets get 25ms delay, slice packets get 12ms
                const isSetup = i < 6 || i === total - 1;
                const delay = isSetup ? 25 : 12;

                await this.writePacket(pkt, delay);

                if (onProgress) {
                    onProgress(i + 1, total, Math.round(((i + 1) / total) * 100));
                }
            }
        } finally {
            this.isPrinting = false;
        }
    }

    cancel() {
        this.cancelRequested = true;
    }

    async disconnect() {
        if (this.type === 'ble') {
            if (this.notifyChar) {
                try {
                    await this.notifyChar.stopNotifications();
                } catch (_) {}
            }
            if (this.device && this.device.gatt && this.device.gatt.connected) {
                try { this.device.gatt.disconnect(); } catch (_) {}
            } else if (this.server && this.server.connected) {
                try { this.server.disconnect(); } catch (_) {}
            }
        } else if (this.type === 'serial' && this.serialPort) {
            if (this.serialWriter) {
                try { await this.serialWriter.close(); } catch (_) {}
                this.serialWriter = null;
            }
            try { await this.serialPort.close(); } catch (_) {}
            this.serialPort = null;
        }
        this.handleDisconnect();
    }
}
