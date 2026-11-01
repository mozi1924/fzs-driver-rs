/**
 * Browser Hardware Transport for FZS / TeacherSaid Thermal Printers
 * Supports Web Bluetooth (BLE) and Web Serial (USB cable).
 */

export const SERVICE_UUIDS = [
    '0000fff0-0000-1000-8000-00805f9b34fb',
    '0000ffe0-0000-1000-8000-00805f9b34fb',
];

export const WRITE_CHAR_UUIDS = [
    '0000fff1-0000-1000-8000-00805f9b34fb',
    '0000ffe1-0000-1000-8000-00805f9b34fb',
    '0000ff01-0000-1000-8000-00805f9b34fb',
];

export const NOTIFY_CHAR_UUIDS = [
    '0000fff2-0000-1000-8000-00805f9b34fb',
    '0000ffe2-0000-1000-8000-00805f9b34fb',
    '0000ff02-0000-1000-8000-00805f9b34fb',
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
            throw new Error('Web Bluetooth is not supported on this browser. Please use Chrome, Edge, or Android Chrome.');
        }

        const device = await navigator.bluetooth.requestDevice({
            filters: [
                { namePrefix: 'FZS' },
                { namePrefix: 'FZ' },
                { namePrefix: 'TP' },
                { namePrefix: 'Printer' },
                { services: [0xFFF0] },
                { services: [0xFFE0] },
            ],
            optionalServices: SERVICE_UUIDS,
        });

        this.device = device;
        device.addEventListener('gattserverdisconnected', () => this.handleDisconnect());

        const server = await device.gatt.connect();
        this.server = server;

        let primaryService = null;
        for (const uuid of SERVICE_UUIDS) {
            try {
                primaryService = await server.getPrimaryService(uuid);
                if (primaryService) break;
            } catch (_) { }
        }

        if (!primaryService) {
            throw new Error('Could not find compatible thermal printer GATT service (0xFFF0/0xFFE0).');
        }

        // Find write characteristic
        for (const uuid of WRITE_CHAR_UUIDS) {
            try {
                this.writeChar = await primaryService.getCharacteristic(uuid);
                if (this.writeChar) break;
            } catch (_) { }
        }

        if (!this.writeChar) {
            throw new Error('Write characteristic not found on printer.');
        }

        // Find notify characteristic
        for (const uuid of NOTIFY_CHAR_UUIDS) {
            try {
                this.notifyChar = await primaryService.getCharacteristic(uuid);
                if (this.notifyChar) {
                    await this.notifyChar.startNotifications();
                    this.notifyChar.addEventListener('characteristicvaluechanged', (e) => this.handleNotify(e));
                    break;
                }
            } catch (_) { }
        }

        this.type = 'ble';
        this.isConnected = true;
        return {
            name: device.name || 'FZS Thermal Printer',
            type: 'Bluetooth BLE',
        };
    }

    async connectSerial() {
        if (!navigator.serial) {
            throw new Error('Web Serial is not supported on this browser. Please use desktop Chrome or Edge.');
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
            setTimeout(() => { this.bufferPaused = false; }, 800);
        }
        if (this.onStatusChange) {
            this.onStatusChange(val);
        }
    }

    handleDisconnect() {
        this.isConnected = false;
        this.isPrinting = false;
        this.writeChar = null;
        this.notifyChar = null;
        this.server = null;
        this.device = null;
        if (this.onStatusChange) {
            this.onStatusChange(null);
        }
    }

    async writePacket(packet, delayMs = 12) {
        if (!this.isConnected) throw new Error('Printer disconnected');

        // Wait if printer signaled buffer full
        while (this.bufferPaused) {
            await new Promise((r) => setTimeout(r, 100));
        }

        if (this.type === 'ble') {
            try {
                if (this.writeChar.writeValueWithResponse) {
                    await this.writeChar.writeValueWithResponse(packet);
                } else {
                    await this.writeChar.writeValue(packet);
                }
            } catch (err) {
                // If GATT was busy, wait a moment and retry once
                await new Promise((r) => setTimeout(r, 50));
                await this.writeChar.writeValue(packet);
            }
        } else if (this.type === 'serial') {
            await this.serialWriter.write(packet);
        }

        if (delayMs > 0) {
            await new Promise((r) => setTimeout(r, delayMs));
        }
    }

    async printJob(packets, onProgress) {
        if (!this.isConnected) throw new Error('Not connected to any printer.');
        this.isPrinting = true;
        this.cancelRequested = false;

        const total = packets.length;
        try {
            for (let i = 0; i < total; i++) {
                if (this.cancelRequested) {
                    throw new Error('Print job cancelled by user.');
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
        if (this.type === 'ble' && this.device && this.device.gatt.connected) {
            this.device.gatt.disconnect();
        } else if (this.type === 'serial' && this.serialPort) {
            if (this.serialWriter) {
                await this.serialWriter.close();
                this.serialWriter = null;
            }
            await this.serialPort.close();
            this.serialPort = null;
        }
        this.handleDisconnect();
    }
}
