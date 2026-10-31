import {
  cloneElement,
  isValidElement,
  ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import noop from "@/utils/noop";

interface Props<Value> {
  value?: Value;
  valueProps?: string;
  onChangeProps?: string;
  waitTime?: number;
  onChange?: (value: Value) => void;
  onFormat?: (...args: any[]) => Value;
  onGuard?: (value: Value, oldValue: Value) => Promise<void>;
  onCatch?: (error: Error) => void;
  children: ReactNode;
}

export function GuardState<T>(props: Props<T>) {
  const {
    value,
    children,
    valueProps = "value",
    onChangeProps = "onChange",
    waitTime = 0,
    onGuard = noop,
    onCatch = noop,
    onChange = noop,
    onFormat = (v: T) => v,
  } = props;

  const lockRef = useRef(false);
  const saveRef = useRef(value);
  const lastRef = useRef(0);
  const timeRef = useRef<any>();
  const mountedRef = useRef(true);
  const [pending, setPending] = useState<{ value: T } | null>(null);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      clearTimeout(timeRef.current);
    };
  }, []);

  if (!isValidElement(children)) {
    return children as any;
  }

  const childProps = { ...children.props };

  childProps[valueProps] = pending ? pending.value : value;
  childProps[onChangeProps] = async (...args: any[]) => {
    // 多次操作无效
    if (lockRef.current) return;

    lockRef.current = true;

    try {
      const newValue = (onFormat as any)(...args);
      // 乐观值只属于当前控件，不写入代表后端已提交状态的查询缓存。
      setPending({ value: newValue });

      const now = Date.now();

      // 防抖期间保留本轮开始前的值，供业务校验使用。
      if (waitTime <= 0 || now - lastRef.current >= waitTime) {
        saveRef.current = value;
      }

      lastRef.current = now;

      if (waitTime <= 0) {
        await onGuard(newValue, value!);
        if (mountedRef.current) onChange(newValue);
      } else {
        // 防抖只延迟请求；进入请求后保持互斥，避免旧请求结束覆盖较新输入。
        clearTimeout(timeRef.current);

        timeRef.current = setTimeout(async () => {
          lockRef.current = true;
          try {
            await onGuard(newValue, saveRef.current!);
            if (mountedRef.current) onChange(newValue);
          } catch (err: any) {
            if (mountedRef.current) onCatch(err);
          } finally {
            lockRef.current = false;
            if (mountedRef.current) setPending(null);
          }
        }, waitTime);
      }
    } catch (err: any) {
      if (mountedRef.current) onCatch(err);
    }
    lockRef.current = false;
    if (waitTime <= 0 && mountedRef.current) setPending(null);
  };
  return cloneElement(children, childProps);
}
