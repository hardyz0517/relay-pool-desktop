import { RefreshCw } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/Card";
import { unknownErrorMessage } from "@/lib/bridge/errorMessage";
import { restartApplication } from "@/lib/bridge/generated";

export function IncompatibleRuntimeScreen({
  error,
  onRetry,
}: {
  error: unknown;
  onRetry: () => void;
}) {
  const [isRestarting, setIsRestarting] = useState(false);

  async function restartRuntime() {
    setIsRestarting(true);
    try {
      await restartApplication();
    } catch {
      // A successful restart can close the IPC channel before the promise resolves.
      setIsRestarting(false);
    }
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-app px-6 text-foreground">
      <Card className="w-full max-w-[640px] p-6">
        <p className="text-sm font-semibold text-danger-foreground">Incompatible desktop runtime</p>
        <p className="mt-2 text-sm text-muted-foreground">
          Relay Pool stopped before loading business data because the bundled frontend and desktop IPC contract do not match.
        </p>
        <pre className="mt-4 max-h-40 overflow-auto rounded-[var(--surface-radius)] border border-border bg-muted px-3 py-2 text-xs text-muted-foreground">
          {unknownErrorMessage(error)}
        </pre>
        <div className="mt-4 flex flex-wrap gap-2">
          <Button variant="secondary" onClick={() => void restartRuntime()} disabled={isRestarting}>
            <RefreshCw className={`h-4 w-4 ${isRestarting ? "animate-spin" : ""}`} />
            {isRestarting ? "Restarting runtime" : "Restart desktop runtime"}
          </Button>
          <Button variant="ghost" onClick={onRetry} disabled={isRestarting}>Retry handshake</Button>
        </div>
      </Card>
    </main>
  );
}
