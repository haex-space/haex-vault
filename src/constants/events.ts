/**
 * Tauri Event Names
 * Diese Konstanten werden aus eventNames.json generiert und mit dem Backend synchronisiert
 */

import eventNames from './eventNames.json'

// Extension Events
export const EXTENSION_WINDOW_CLOSED = eventNames.extension.windowClosed
export const EXTENSION_AUTO_START_REQUEST = eventNames.extension.autoStartRequest
export const EXTENSION_READY = eventNames.extension.ready
