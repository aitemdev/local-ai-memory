#!/usr/bin/env node
process.env.NODE_NO_WARNINGS ||= "1";
const emitWarning = process.emitWarning.bind(process);
process.emitWarning = (warning, ...args) => {
  const message = typeof warning === "string" ? warning : warning?.message;
  if (message?.includes("SQLite is an experimental feature")) return;
  emitWarning(warning, ...args);
};

const { main } = await import("../src/cli.js");

main(process.argv.slice(2)).catch((error) => {
  console.error(error?.stack || error?.message || String(error));
  process.exitCode = 1;
});
