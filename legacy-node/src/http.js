import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { addPath, searchMemory, status } from "./indexer.js";

export function serveHttp({ port = 3737 } = {}) {
  const publicDir = path.resolve("public");
  const server = http.createServer(async (req, res) => {
    try {
      const url = new URL(req.url, `http://${req.headers.host}`);
      if (url.pathname === "/api/search") {
        const results = await searchMemory(url.searchParams.get("q") || "", {
          budget: url.searchParams.get("budget") || "normal"
        });
        return json(res, results);
      }
      if (url.pathname === "/api/status") return json(res, status());
      if (url.pathname === "/api/add" && req.method === "POST") {
        const body = await readJson(req);
        return json(res, await addPath(body.path));
      }
      const filePath = path.join(publicDir, url.pathname === "/" ? "index.html" : url.pathname);
      if (!filePath.startsWith(publicDir) || !fs.existsSync(filePath)) {
        res.writeHead(404);
        res.end("Not found");
        return;
      }
      res.writeHead(200, { "content-type": contentType(filePath) });
      fs.createReadStream(filePath).pipe(res);
    } catch (error) {
      res.writeHead(500, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: error.message }));
    }
  });
  server.listen(port, () => {
    console.log(`Local AI Memory UI: http://localhost:${port}`);
  });
}

function json(res, value) {
  res.writeHead(200, { "content-type": "application/json" });
  res.end(JSON.stringify(value, null, 2));
}

function readJson(req) {
  return new Promise((resolve, reject) => {
    let body = "";
    req.on("data", (chunk) => body += chunk);
    req.on("end", () => {
      try {
        resolve(JSON.parse(body || "{}"));
      } catch (error) {
        reject(error);
      }
    });
  });
}

function contentType(filePath) {
  if (filePath.endsWith(".css")) return "text/css";
  if (filePath.endsWith(".js")) return "text/javascript";
  return "text/html";
}
