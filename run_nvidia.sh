#!/usr/bin/env bash
# Spuštění hry na diskrétní NVIDIA grafice přes Vulkan (Zink) s aktivním vkBasalt
__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia MESA_LOADER_DRIVER_OVERRIDE=zink ENABLE_VKBASALT=1 cargo run "$@"
