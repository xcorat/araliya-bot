import { readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

async function main() {
    // Path to the Rust crate's Cargo.toml.  The workspace root is three
    // levels up from this script (frontend/svui/scripts).
    const cargoPath = resolve(__dirname, '..', '..', '..', 'crates', 'araliya-bot', 'Cargo.toml');
    const text = await readFile(cargoPath, 'utf8');
    const m = text.match(/^version\s*=\s*"([^"]+)"/m);
    if (!m) {
        console.error('unable to extract version from Cargo.toml');
        process.exit(1);
    }
    const version = m[1];

    // Write a simple environment file that Vite will pick up.  We rewrite it
    // on every run so that the UI always reflects the current Cargo version.
    const envPath = resolve(__dirname, '..', '..', '.env');
    await writeFile(envPath, `VITE_APP_VERSION=${version}\n`);
    console.log(`synced frontend version ${version}`);
}

main().catch((e) => {
    console.error(e);
    process.exit(1);
});
