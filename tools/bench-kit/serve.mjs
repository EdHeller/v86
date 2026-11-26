// serves the bench kit and appends POSTed log lines to a file: node ffserve.mjs <dir> <port> <log>
import http from "node:http"; import fs from "node:fs"; import path from "node:path";
const [dir, port, logfile] = process.argv.slice(2);
const types = { ".html": "text/html", ".mjs": "text/javascript", ".js": "text/javascript", ".wasm": "application/wasm" };
http.createServer((req, res) => {
    if(req.method === "POST") { let b = ""; req.on("data", d => b += d); req.on("end", () => { fs.appendFileSync(logfile, b + "\n"); res.end(); }); return; }
    const f = path.join(dir, decodeURIComponent(req.url.split("?")[0]));
    if(!f.startsWith(dir) || !fs.existsSync(f) || fs.statSync(f).isDirectory()) { res.statusCode = 404; return res.end(); }
    res.setHeader("Content-Type", types[path.extname(f)] || "application/octet-stream");
    fs.createReadStream(f).pipe(res);
}).listen(+port, "127.0.0.1");
