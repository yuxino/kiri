#!/usr/bin/env node

import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const currentFile = fileURLToPath(import.meta.url);
const scriptsDir = path.dirname(currentFile);
const repositoryRoot = path.dirname(scriptsDir);

export function tauriCommand(args) {
  return args.find((argument) => !argument.startsWith("-"));
}

export function tauriTarget(args) {
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--") {
      return undefined;
    }
    if (argument === "--target" || argument === "-t") {
      return args[index + 1];
    }
    if (argument.startsWith("--target=")) {
      return argument.slice("--target=".length);
    }
  }
  return undefined;
}

export function parseRustcHost(output) {
  const match = output.match(/^host:\s*(\S+)\s*$/m);
  if (!match) {
    throw new Error("Could not determine the Rust host target from `rustc -vV`.");
  }
  return match[1];
}

export function cargoRunnerEnvironmentKey(host) {
  const normalizedHost = host.toUpperCase().replace(/[^A-Z0-9]/g, "_");
  return `CARGO_TARGET_${normalizedHost}_RUNNER`;
}

export function macosDevEnvironment(baseEnvironment, host, runnerDirectory) {
  return {
    ...baseEnvironment,
    [cargoRunnerEnvironmentKey(host)]: "kiri-macos-dev-runner",
    PATH: [runnerDirectory, baseEnvironment.PATH].filter(Boolean).join(path.delimiter),
  };
}

function rustHost(environment) {
  const result = spawnSync("rustc", ["-vV"], {
    encoding: "utf8",
    env: environment,
  });

  if (result.error) {
    throw new Error(`Could not run rustc: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || "`rustc -vV` failed.");
  }
  return parseRustcHost(result.stdout);
}

function runTauri(args, environment) {
  const tauriEntry = path.join(
    repositoryRoot,
    "node_modules",
    "@tauri-apps",
    "cli",
    "tauri.js",
  );
  const child = spawn(process.execPath, [tauriEntry, ...args], {
    cwd: process.cwd(),
    env: environment,
    stdio: "inherit",
  });

  const forwardedSignals = process.platform === "win32"
    ? ["SIGINT", "SIGTERM"]
    : ["SIGINT", "SIGTERM", "SIGHUP"];
  const handlers = new Map();
  for (const signal of forwardedSignals) {
    const handler = () => {
      if (!child.killed) child.kill(signal);
    };
    handlers.set(signal, handler);
    process.on(signal, handler);
  }

  child.on("error", (error) => {
    console.error(`Could not start the Tauri CLI: ${error.message}`);
    process.exitCode = 1;
  });

  child.on("exit", (code, signal) => {
    for (const [registeredSignal, handler] of handlers) {
      process.off(registeredSignal, handler);
    }
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exitCode = code ?? 1;
  });
}

export function main(args = process.argv.slice(2)) {
  let environment = process.env;
  if (process.platform === "darwin" && tauriCommand(args) === "dev") {
    const target = tauriTarget(args) || rustHost(process.env);
    environment = macosDevEnvironment(
      process.env,
      target,
      scriptsDir,
    );
  }
  runTauri(args, environment);
}

if (process.argv[1] && path.resolve(process.argv[1]) === currentFile) {
  try {
    main();
  } catch (error) {
    console.error(`kiri dev setup failed: ${error.message}`);
    process.exitCode = 1;
  }
}
