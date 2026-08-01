import http from "node:http";

const port = Number.parseInt(process.env.RELAY_POOL_ROUTING_FIXTURE_PORT ?? "18181", 10);
const host = "127.0.0.1";
const model = "routing-fixture-chat";
const embeddingModel = "routing-fixture-embedding";

if (!Number.isInteger(port) || port < 1024 || port > 65535) {
  throw new Error("RELAY_POOL_ROUTING_FIXTURE_PORT must be an integer in 1024..65535");
}

const server = http.createServer(async (request, response) => {
  try {
    await handleRequest(request, response);
  } catch {
    sendJson(response, 500, {
      error: {
        message: "fixture_internal_error",
        type: "fixture_error",
        code: "fixture_internal_error",
      },
    });
  }
});

server.listen(port, host, () => {
  console.log(`Routing workspace fixture server listening at http://${host}:${port}/v1`);
  console.log("Use only synthetic keys and requests. This server does not log request bodies or headers.");
});

process.on("SIGINT", () => server.close(() => process.exit(0)));
process.on("SIGTERM", () => server.close(() => process.exit(0)));

async function handleRequest(request, response) {
  setCorsHeaders(response);
  const url = new URL(request.url ?? "/", `http://${host}:${port}`);

  if (request.method === "OPTIONS") {
    response.writeHead(204);
    response.end();
    return;
  }

  if (request.method === "GET" && url.pathname === "/v1/models") {
    sendJson(response, 200, {
      object: "list",
      data: [
        { id: model, object: "model", created: 1_775_000_000, owned_by: "relay-pool-fixture" },
        { id: embeddingModel, object: "model", created: 1_775_000_000, owned_by: "relay-pool-fixture" },
      ],
    });
    return;
  }

  if (request.method === "POST" && url.pathname === "/v1/chat/completions") {
    const body = await readJsonBody(request);
    if (body?.stream === true) {
      sendChatCompletionStream(response);
    } else {
      sendJson(response, 200, {
        id: "chatcmpl-routing-fixture",
        object: "chat.completion",
        created: 1_775_000_000,
        model: body?.model ?? model,
        choices: [
          {
            index: 0,
            message: { role: "assistant", content: "routing fixture chat response" },
            finish_reason: "stop",
          },
        ],
        usage: { prompt_tokens: 5, completion_tokens: 4, total_tokens: 9 },
      });
    }
    return;
  }

  if (request.method === "POST" && url.pathname === "/v1/responses") {
    const body = await readJsonBody(request);
    sendJson(response, 200, {
      id: "resp-routing-fixture",
      object: "response",
      created_at: 1_775_000_000,
      model: body?.model ?? model,
      status: "completed",
      output: [
        {
          id: "msg-routing-fixture",
          type: "message",
          role: "assistant",
          content: [{ type: "output_text", text: "routing fixture responses output" }],
        },
      ],
      usage: { input_tokens: 5, output_tokens: 4, total_tokens: 9 },
    });
    return;
  }

  if (request.method === "POST" && url.pathname === "/v1/embeddings") {
    const body = await readJsonBody(request);
    sendJson(response, 200, {
      object: "list",
      model: body?.model ?? embeddingModel,
      data: [{ object: "embedding", index: 0, embedding: [0.01, 0.02, 0.03, 0.04] }],
      usage: { prompt_tokens: 3, total_tokens: 3 },
    });
    return;
  }

  sendJson(response, 404, {
    error: {
      message: "fixture_endpoint_not_found",
      type: "fixture_error",
      code: "fixture_endpoint_not_found",
    },
  });
}

function sendChatCompletionStream(response) {
  response.writeHead(200, {
    "content-type": "text/event-stream; charset=utf-8",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  response.write(
    `data: ${JSON.stringify({
      id: "chatcmpl-routing-fixture-stream",
      object: "chat.completion.chunk",
      created: 1_775_000_000,
      model,
      choices: [{ index: 0, delta: { role: "assistant", content: "routing " }, finish_reason: null }],
    })}\n\n`,
  );
  response.write(
    `data: ${JSON.stringify({
      id: "chatcmpl-routing-fixture-stream",
      object: "chat.completion.chunk",
      created: 1_775_000_000,
      model,
      choices: [{ index: 0, delta: { content: "fixture stream" }, finish_reason: "stop" }],
    })}\n\n`,
  );
  response.end("data: [DONE]\n\n");
}

function sendJson(response, statusCode, body) {
  response.writeHead(statusCode, { "content-type": "application/json; charset=utf-8" });
  response.end(JSON.stringify(body));
}

function setCorsHeaders(response) {
  response.setHeader("access-control-allow-origin", "*");
  response.setHeader("access-control-allow-methods", "GET,POST,OPTIONS");
  response.setHeader("access-control-allow-headers", "authorization,content-type");
}

async function readJsonBody(request) {
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(chunk);
  }
  if (chunks.length === 0) return null;
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    return null;
  }
}
