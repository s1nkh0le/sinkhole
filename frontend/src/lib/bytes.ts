/**
 * Base64 decoding + hex/text view formatting for raw_b64 fields.
 */

export function decodeBase64(b64: string): Uint8Array {
  const binaryStr = atob(b64);
  const bytes = new Uint8Array(binaryStr.length);
  for (let i = 0; i < binaryStr.length; i++) {
    bytes[i] = binaryStr.charCodeAt(i);
  }
  return bytes;
}

export function bytesToHexDump(bytes: Uint8Array, bytesPerRow = 16): string {
  const lines: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += bytesPerRow) {
    const slice = bytes.slice(offset, offset + bytesPerRow);
    
    // Offset column
    const offsetStr = offset.toString(16).padStart(8, '0');
    
    // Hex column
    const hexParts: string[] = [];
    for (let i = 0; i < bytesPerRow; i++) {
      if (i < slice.length) {
        hexParts.push(slice[i].toString(16).padStart(2, '0'));
      } else {
        hexParts.push('  ');
      }
    }
    // Group in pairs of 8 with extra space
    const hexStr = hexParts.slice(0, 8).join(' ') + '  ' + hexParts.slice(8).join(' ');
    
    // ASCII column
    const asciiStr = Array.from(slice)
      .map(b => (b >= 0x20 && b <= 0x7e) ? String.fromCharCode(b) : '.')
      .join('');
    
    lines.push(`${offsetStr}  ${hexStr}  |${asciiStr}|`);
  }
  return lines.join('\n');
}

export function bytesToText(bytes: Uint8Array): string {
  try {
    return new TextDecoder('utf-8', { fatal: false }).decode(bytes);
  } catch {
    return Array.from(bytes)
      .map(b => (b >= 0x20 && b <= 0x7e) ? String.fromCharCode(b) : `\\x${b.toString(16).padStart(2, '0')}`)
      .join('');
  }
}

export function formatByteSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  if (i === 0) return `${bytes} B`;
  const value = bytes / Math.pow(1024, i);
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[i]}`;
}
