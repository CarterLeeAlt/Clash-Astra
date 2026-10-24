import { useEffect, useState } from "react";
import { useLockFn } from "ahooks";
import delayManager, { DELAY_PENDING, DELAY_UNKNOWN } from "@/services/delay";
import { useVerge } from "@/hooks/use-verge";

/** 订阅延迟状态，并在组件卸载时释放 delayManager 监听。 */
export const useProxyDelay = (proxy: IProxyItem, groupName: string) => {
  const [delay, setDelay] = useState(DELAY_UNKNOWN);
  const { verge } = useVerge();
  const rawTimeout = Number(verge?.default_latency_timeout);
  const timeout =
    Number.isInteger(rawTimeout) && rawTimeout > 0 ? rawTimeout : 3000;

  useEffect(() => {
    delayManager.setListener(proxy.name, groupName, setDelay);

    return () => {
      delayManager.removeListener(proxy.name, groupName, setDelay);
    };
  }, [proxy.name, groupName]);

  useEffect(() => {
    if (!proxy) return;
    setDelay(delayManager.getDelayFix(proxy, groupName));
  }, [proxy, groupName]);

  const onDelay = useLockFn(async () => {
    setDelay(DELAY_PENDING);
    setDelay(await delayManager.checkDelay(proxy.name, groupName, timeout));
  });

  return { delay, onDelay, timeout };
};
