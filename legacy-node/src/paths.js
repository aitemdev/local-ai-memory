import path from "node:path";

export function memoryHome() {
  return path.resolve(process.env.MEM_HOME || path.join(process.cwd(), ".memoria"));
}

export function dataPaths(base = memoryHome()) {
  return {
    base,
    db: path.join(base, "memory.sqlite"),
    originals: path.join(base, "originals"),
    canonical: path.join(base, "canonical"),
    logs: path.join(base, "logs")
  };
}
