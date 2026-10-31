import { useEffect, useState } from "react";

/**
 * 窗口宽度只用于代理列表的列数断点。拖动窗口边缘时逐像素 setState 会
 * 让整页反复重渲染，这里做尾沿防抖，拖动过程中不更新。
 */
export const useWindowWidth = () => {
  const [width, setWidth] = useState(() => document.body.clientWidth);

  useEffect(() => {
    let timer: number | undefined;

    const handleResize = () => {
      if (timer !== undefined) window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        setWidth(document.body.clientWidth);
        timer = undefined;
      }, 120);
    };

    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, []);

  return { width };
};
