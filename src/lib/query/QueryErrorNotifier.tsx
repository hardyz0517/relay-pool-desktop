import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useToast } from "@/components/ui";

export function QueryErrorNotifier() {
  const queryClient = useQueryClient();
  const toast = useToast();
  const lastNotifiedAt = useRef(new Map<string, number>());

  useEffect(
    () =>
      queryClient.getQueryCache().subscribe((event) => {
        if (event.type !== "updated" || event.action.type !== "error") return;

        const { errorUpdatedAt } = event.query.state;
        if (lastNotifiedAt.current.get(event.query.queryHash) === errorUpdatedAt) return;

        lastNotifiedAt.current.set(event.query.queryHash, errorUpdatedAt);
        toast.error("数据刷新失败", "已保留最近一次成功数据，请稍后重试。");
      }),
    [queryClient, toast],
  );

  return null;
}
