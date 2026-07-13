import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useToast } from "@/components/ui";
import { createQueryErrorNotificationCycle } from "./queryErrorNotificationCycle";

export function QueryErrorNotifier() {
  const queryClient = useQueryClient();
  const toast = useToast();
  const notificationCycle = useRef(createQueryErrorNotificationCycle());

  useEffect(
    () =>
      queryClient.getQueryCache().subscribe((event) => {
        if (event.type === "removed") {
          notificationCycle.current.reset(event.query.queryHash);
          return;
        }
        if (event.type !== "updated") return;

        if (event.action.type === "success") {
          notificationCycle.current.reset(event.query.queryHash);
          return;
        }
        if (event.action.type !== "error") return;
        if (!notificationCycle.current.shouldNotify(event.query.queryHash)) return;

        toast.error("数据刷新失败", "已保留最近一次成功数据，请稍后重试。");
      }),
    [queryClient, toast],
  );

  return null;
}
