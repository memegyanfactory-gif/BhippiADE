import {
  RuntimeWorkerSession,
  type RuntimeWorkerEnvelope,
  type RuntimeWorkerRequest,
  type RuntimeWorkerResponse,
} from "./runtimeWorkerSession.ts";

type WorkerScope = {
  onmessage: ((event: MessageEvent<RuntimeWorkerEnvelope<RuntimeWorkerRequest>>) => void) | null;
  postMessage(message: RuntimeWorkerEnvelope<RuntimeWorkerResponse>): void;
  close(): void;
};

const scope = self as unknown as WorkerScope;
let session: RuntimeWorkerSession | null = null;

scope.onmessage = (event) => {
  session ??= new RuntimeWorkerSession(event.data.sessionNonce);
  const response = session.handle(event.data);
  scope.postMessage(response);
  if (
    response.payload.kind === "fault" ||
    response.payload.kind === "stopped" ||
    response.payload.kind === "playtest_report"
  ) {
    scope.close();
  }
};
