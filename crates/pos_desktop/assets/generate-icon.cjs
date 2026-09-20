// Regenerate the Windows icon from the editable vector: node generate-icon.cjs
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');

async function main() {
  const sizes = [16, 24, 32, 48, 64, 128, 256];
  const source = path.join(__dirname, 'kay-simple.svg');
  const images = [];
  for (const size of sizes) {
    images.push(await sharp(source, { density: 384 }).resize(size, size).png().toBuffer());
  }
  const header = Buffer.alloc(6 + 16 * sizes.length);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);
  let offset = header.length;
  sizes.forEach((size, index) => {
    const entry = 6 + 16 * index;
    header[entry] = header[entry + 1] = size === 256 ? 0 : size;
    header.writeUInt16LE(1, entry + 4);
    header.writeUInt16LE(32, entry + 6);
    header.writeUInt32LE(images[index].length, entry + 8);
    header.writeUInt32LE(offset, entry + 12);
    offset += images[index].length;
  });
  fs.writeFileSync(path.join(__dirname, 'kay-simple.ico'), Buffer.concat([header, ...images]));
  fs.writeFileSync(path.join(__dirname, 'kay-simple.png'), images[images.length - 1]);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
