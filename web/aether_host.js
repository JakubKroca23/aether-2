"use strict";

// Host hooks for the wasm build. Registered before `load()` so they exist
// as env imports when the module is instantiated.
const AETHER_SAVE_KEY = "aether.saves.v1";

function aetherBytesToB64(bytes) {
    let bin = "";
    const chunk = 0x8000;
    for (let i = 0; i < bytes.length; i += chunk) {
        bin += String.fromCharCode.apply(
            null,
            bytes.subarray(i, Math.min(i + chunk, bytes.length))
        );
    }
    return btoa(bin);
}

function aetherB64ToBytes(text) {
    const bin = atob(text);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) {
        out[i] = bin.charCodeAt(i);
    }
    return out;
}

let aetherPending = null;

miniquad_add_plugin({
    name: "aether_host",
    version: 1,
    register_plugin: function (importObject) {
        importObject.env.aether_unix_ms = function () {
            return Date.now();
        };
        importObject.env.aether_auto_run = function () {
            try {
                const q = new URLSearchParams(window.location.search);
                return q.has("run") ? 1 : 0;
            } catch (err) {
                return 0;
            }
        };
        importObject.env.aether_storage_len = function () {
            aetherPending = null;
            try {
                const text = localStorage.getItem(AETHER_SAVE_KEY);
                if (!text) {
                    return 0;
                }
                const bytes = aetherB64ToBytes(text);
                aetherPending = bytes;
                return bytes.length;
            } catch (err) {
                console.warn("aether: localStorage read failed", err);
                return 0;
            }
        };
        importObject.env.aether_storage_read = function (ptr, cap) {
            const bytes = aetherPending;
            if (!bytes || cap <= 0) {
                return 0;
            }
            const n = Math.min(bytes.length, cap);
            new Uint8Array(wasm_memory.buffer, ptr, n).set(bytes.subarray(0, n));
            return n;
        };
        importObject.env.aether_storage_write = function (ptr, len) {
            try {
                if (len <= 0) {
                    localStorage.removeItem(AETHER_SAVE_KEY);
                    return 1;
                }
                const bytes = new Uint8Array(wasm_memory.buffer, ptr, len).slice();
                localStorage.setItem(AETHER_SAVE_KEY, aetherBytesToB64(bytes));
                return 1;
            } catch (err) {
                console.warn("aether: localStorage write failed", err);
                return 0;
            }
        };
    },
});
