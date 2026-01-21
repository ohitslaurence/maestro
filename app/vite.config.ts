import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import net from "node:net";
import readline from "node:readline";
import type { IncomingMessage, ServerResponse } from "node:http";

const host = process.env.TAURI_DEV_HOST;
const DAEMON_RPC_PATH = "/__daemon__/rpc";
const DAEMON_EVENTS_PATH = "/__daemon__/events";

type RpcRequest = {
  host: string;
  port: number;
  token: string;
  method: string;
  params?: Record<string, unknown> | null;
};

function readBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf-8")));
    req.on("error", (error) => reject(error));
  });
}

function sendJson(res: ServerResponse, status: number, payload: unknown) {
  res.statusCode = status;
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify(payload));
}

function parseRpcRequest(body: string): RpcRequest {
  const parsed = JSON.parse(body) as RpcRequest;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("invalid_payload");
  }

  const host = typeof parsed.host === "string" ? parsed.host : "";
  const port = Number(parsed.port);
  const token = typeof parsed.token === "string" ? parsed.token : "";
  const method = typeof parsed.method === "string" ? parsed.method : "";

  if (!host || !Number.isFinite(port) || !method) {
    throw new Error("invalid_payload");
  }

  return {
    host,
    port,
    token,
    method,
    params: parsed.params ?? null,
  };
}

function sendRpc(request: RpcRequest): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(
      { host: request.host, port: request.port },
      () => {
        const auth = {
          id: 1,
          method: "auth",
          params: { token: request.token },
        };
        const command = {
          id: 2,
          method: request.method,
          params: request.params ?? {},
        };
        socket.write(`${JSON.stringify(auth)}\n`);
        socket.write(`${JSON.stringify(command)}\n`);
      },
    );

    const reader = readline.createInterface({ input: socket });
    let done = false;
    let authed = false;
    const timeout = setTimeout(() => {
      fail({ code: "rpc_timeout", message: "RPC timeout" });
    }, 30_000);

    const cleanup = () => {
      if (done) {
        return;
      }
      done = true;
      clearTimeout(timeout);
      reader.close();
      socket.destroy();
    };

    const fail = (error: { code: string; message: string }) => {
      cleanup();
      reject(error);
    };

    reader.on("line", (line) => {
      if (!line.trim()) {
        return;
      }
      let message: any = null;
      try {
        message = JSON.parse(line);
      } catch (error) {
        fail({ code: "invalid_json", message: String(error) });
        return;
      }

      if (message?.id === 1) {
        if (message?.result?.ok) {
          authed = true;
          return;
        }
        const code = message?.error?.code ?? "auth_failed";
        const msg = message?.error?.message ?? "Auth failed";
        fail({ code, message: msg });
        return;
      }

      if (message?.id === 2) {
        if (message?.error) {
          const code = message?.error?.code ?? "rpc_error";
          const msg = message?.error?.message ?? "RPC error";
          fail({ code, message: msg });
          return;
        }
        cleanup();
        resolve(message?.result);
        return;
      }

      if (!authed) {
        return;
      }
    });

    socket.on("error", (error) => {
      fail({ code: "rpc_connection", message: error.message });
    });

    reader.on("error", (error) => {
      fail({ code: "rpc_connection", message: String(error) });
    });

    socket.on("close", () => {
      if (!done) {
        fail({ code: "rpc_connection", message: "Connection closed" });
      }
    });
  });
}

function handleEvents(req: IncomingMessage, res: ServerResponse) {
  const url = new URL(req.url ?? "", "http://localhost");
  const host = url.searchParams.get("host") ?? "";
  const port = Number(url.searchParams.get("port"));
  const token = url.searchParams.get("token") ?? "";

  if (!host || !Number.isFinite(port)) {
    res.statusCode = 400;
    res.end("Missing host or port");
    return;
  }

  res.writeHead(200, {
    "Content-Type": "text/event-stream",
    "Cache-Control": "no-cache",
    Connection: "keep-alive",
  });
  res.write("retry: 1000\n\n");

  const socket = net.createConnection({ host, port }, () => {
    const auth = {
      id: 1,
      method: "auth",
      params: { token },
    };
    socket.write(`${JSON.stringify(auth)}\n`);
  });

  const reader = readline.createInterface({ input: socket });
  let authed = false;
  let closed = false;

  const cleanup = () => {
    if (closed) {
      return;
    }
    closed = true;
    reader.close();
    socket.destroy();
  };

  reader.on("line", (line) => {
    if (!line.trim()) {
      return;
    }
    let message: any = null;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }

    if (message?.id === 1) {
      if (message?.result?.ok) {
        authed = true;
      } else {
        const errorPayload = JSON.stringify({
          error: message?.error ?? { code: "auth_failed", message: "Auth failed" },
        });
        res.write(`data: ${errorPayload}\n\n`);
        cleanup();
      }
      return;
    }

    if (!authed) {
      return;
    }

    if (message?.method) {
      res.write(`data: ${JSON.stringify(message)}\n\n`);
    }
  });

  socket.on("error", (error) => {
    const errorPayload = JSON.stringify({
      error: { code: "event_stream", message: error.message },
    });
    res.write(`data: ${errorPayload}\n\n`);
    cleanup();
  });

  reader.on("error", (error) => {
    const errorPayload = JSON.stringify({
      error: { code: "event_stream", message: String(error) },
    });
    res.write(`data: ${errorPayload}\n\n`);
    cleanup();
  });

  socket.on("close", () => {
    cleanup();
  });

  req.on("close", () => {
    cleanup();
  });
}

function daemonBridge(): Plugin {
  return {
    name: "maestro-daemon-bridge",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const url = req.url ?? "";
        if (url.startsWith(DAEMON_RPC_PATH)) {
          if (req.method !== "POST") {
            sendJson(res, 405, { error: { code: "method_not_allowed" } });
            return;
          }

          readBody(req)
            .then((body) => parseRpcRequest(body))
            .then((rpc) => sendRpc(rpc))
            .then((result) => sendJson(res, 200, { result }))
            .catch((error) => {
              const code = error?.code ?? "rpc_error";
              const message = error?.message ?? String(error ?? "RPC error");
              sendJson(res, 500, { error: { code, message } });
            });
          return;
        }

        if (url.startsWith(DAEMON_EVENTS_PATH)) {
          handleEvents(req, res);
          return;
        }

        next();
      });
    },
  };
}

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), daemonBridge()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
