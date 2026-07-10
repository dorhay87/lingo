// Renders promo/promo.html headlessly in Microsoft Edge (ships with Windows)
// and saves promo.png (1.25x) at the repo root.
// Run: npm run promo
"use strict";
const { execFileSync } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

const W = 1200; // must match .promo-root width in promo.css
const H = 700; //  min-height in promo.css
const SCALE = 1.25;
const PAD = 80; // extra viewport height; headless screenshots paint garbage
//                 in the bottom band, so capture tall and crop it away

const EDGE_PATHS = [
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
];
const edge = EDGE_PATHS.find(fs.existsSync);
if (!edge) {
  console.error("Microsoft Edge not found - install it or adjust EDGE_PATHS.");
  process.exit(1);
}

const page = path.join(__dirname, "..", "promo", "promo.html");
const out = path.join(__dirname, "..", "promo.png");
const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "lingo-promo-"));
const raw = path.join(tmpDir, "raw.png");

try {
  execFileSync(
    edge,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      `--user-data-dir=${path.join(tmpDir, "profile")}`,
      `--window-size=${W},${H + PAD}`,
      `--force-device-scale-factor=${SCALE}`,
      `--screenshot=${raw}`,
      "file:///" + page.replace(/\\/g, "/"),
    ],
    { stdio: "pipe", timeout: 60_000 },
  );

  // Crop the padding band off the bottom (System.Drawing - no npm deps).
  const cw = Math.round(W * SCALE);
  const ch = Math.round(H * SCALE);
  execFileSync(
    "powershell",
    [
      "-NoProfile",
      "-Command",
      `
    Add-Type -AssemblyName System.Drawing;
    $src = [System.Drawing.Image]::FromFile('${raw}');
    $rect = New-Object System.Drawing.Rectangle 0, 0, ${cw}, ${ch};
    $dst = ([System.Drawing.Bitmap]$src).Clone($rect, $src.PixelFormat);
    $src.Dispose();
    $dst.Save('${out}', [System.Drawing.Imaging.ImageFormat]::Png);
    $dst.Dispose();
  `,
    ],
    { stdio: "pipe", timeout: 30_000 },
  );
} finally {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}

// Report the PNG's real dimensions from its IHDR header.
const buf = fs.readFileSync(out);
console.log("wrote", out, {
  width: buf.readUInt32BE(16),
  height: buf.readUInt32BE(20),
});
