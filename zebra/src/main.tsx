import { Command } from "@tauri-apps/plugin-shell";
import { warn, debug, trace, info, error } from '@tauri-apps/plugin-log';

function forwardConsole(
  fnName: 'log' | 'debug' | 'info' | 'warn' | 'error',
  logger: (message: string) => Promise<void>
) {
  const original = console[fnName];
  console[fnName] = (message) => {
    original(message);
    logger(message);
  };
}

forwardConsole('log', trace);
forwardConsole('debug', debug);
forwardConsole('info', info);
forwardConsole('warn', warn);
forwardConsole('error', error);

console.info("start");

let child;

const command = Command.sidecar("./bin/horsed", ["--help"], {});
command.on("close", data => {
  console.info(`close: ${data}`);
});

command.on("error", err => {
  console.info(`error: ${err}`);
});

command.stdout.on("data", line => {
  console.info(`stdout: ${line}`);
});

command.stderr.on("data", line => {
  console.info(`stderr: ${line}`);
});

try {
  child = await command.execute();
  console.info(child.stdout);
  console.info(child.stderr);
} catch (err) {
  console.info(`spawn error: ${err}`);
}
