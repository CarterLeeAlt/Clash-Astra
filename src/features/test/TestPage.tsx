import { useEffect, useMemo, useRef, useState } from "react";
import { useVerge } from "@/hooks/use-verge";
import { Box, Button, Grid } from "@mui/material";
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
} from "@dnd-kit/sortable";

import { useTranslation } from "react-i18next";
import { BasePage, formatNoticeMessage, Notice } from "@/components/base";
import { TestViewer, TestViewerRef } from "./components/test-viewer";
import { TestItem } from "./components/test-item";
import AppleIcon from "@/assets/image/apple.svg";
import YouTubeIcon from "@/assets/image/youtube.svg";
import ChatGptIcon from "@/assets/image/chatgpt.svg";
import AnthropicIcon from "@/assets/image/anthropic.svg";
import GrokIcon from "@/assets/image/grok.png";
import { normalizeTestList } from "@/features/test/normalize";
import { triggerAutoSiteTest } from "@/services/cmds";
import {
  clearSiteTestResults,
  getFreshSiteTestResults,
  SITE_TEST_RESULT_TTL_MS,
  subscribeSiteTestResults,
  type SiteTestResults,
} from "@/features/test/site-test-results";

const DEFAULT_TEST_ITEMS: Record<string, IVergeTestItem> = {
  "builtin-apple": {
    uid: "builtin-apple",
    name: "Apple",
    url: "https://www.apple.com",
    builtIn: true,
    icon: AppleIcon,
  },
  "builtin-github": {
    uid: "builtin-github",
    name: "GitHub",
    url: "https://github.com",
    builtIn: true,
    icon: `<svg width="98" height="96" xmlns="http://www.w3.org/2000/svg"><path fill-rule="evenodd" clip-rule="evenodd" d="M48.854 0C21.839 0 0 22 0 49.217c0 21.756 13.993 40.172 33.405 46.69 2.427.49 3.316-1.059 3.316-2.362 0-1.141-.08-5.052-.08-9.127-13.59 2.934-16.42-5.867-16.42-5.867-2.184-5.704-5.42-7.17-5.42-7.17-4.448-3.015.324-3.015.324-3.015 4.934.326 7.523 5.052 7.523 5.052 4.367 7.496 11.404 5.378 14.235 4.074.404-3.178 1.699-5.378 3.074-6.6-10.839-1.141-22.243-5.378-22.243-24.283 0-5.378 1.94-9.778 5.014-13.2-.485-1.222-2.184-6.275.486-13.038 0 0 4.125-1.304 13.426 5.052a46.97 46.97 0 0 1 12.214-1.63c4.125 0 8.33.571 12.213 1.63 9.302-6.356 13.427-5.052 13.427-5.052 2.67 6.763.97 11.816.485 13.038 3.155 3.422 5.015 7.822 5.015 13.2 0 18.905-11.404 23.06-22.324 24.283 1.78 1.548 3.316 4.481 3.316 9.126 0 6.6-.08 11.897-.08 13.526 0 1.304.89 2.853 3.316 2.364 19.412-6.52 33.405-24.935 33.405-46.691C97.707 22 75.788 0 48.854 0z" fill="#000000"/></svg>`,
  },
  "builtin-google": {
    uid: "builtin-google",
    name: "Google",
    url: "https://www.google.com",
    builtIn: true,
    icon: `<svg enable-background="new 0 0 48 48" height="48" viewBox="0 0 48 48" width="48" xmlns="http://www.w3.org/2000/svg"><path d="m43.611 20.083h-1.611v-.083h-18v8h11.303c-1.649 4.657-6.08 8-11.303 8-6.627 0-12-5.373-12-12s5.373-12 12-12c3.059 0 5.842 1.154 7.961 3.039l5.657-5.657c-3.572-3.329-8.35-5.382-13.618-5.382-11.045 0-20 8.955-20 20s8.955 20 20 20 20-8.955 20-20c0-1.341-.138-2.65-.389-3.917z" fill="#ffc107"/><path d="m6.306 14.691 6.571 4.819c1.778-4.402 6.084-7.51 11.123-7.51 3.059 0 5.842 1.154 7.961 3.039l5.657-5.657c-3.572-3.329-8.35-5.382-13.618-5.382-7.682 0-14.344 4.337-17.694 10.691z" fill="#ff3d00"/><path d="m24 44c5.166 0 9.86-1.977 13.409-5.192l-6.19-5.238c-2.008 1.521-4.504 2.43-7.219 2.43-5.202 0-9.619-3.317-11.283-7.946l-6.522 5.025c3.31 6.477 10.032 10.921 17.805 10.921z" fill="#4caf50"/><path d="m43.611 20.083h-1.611v-.083h-18v8h11.303c-.792 2.237-2.231 4.166-4.087 5.571.001-.001.002-.001.003-.002l6.19 5.238c-.438.398 6.591-4.807 6.591-14.807 0-1.341-.138-2.65-.389-3.917z" fill="#1976d2"/></svg>`,
  },
  "builtin-youtube": {
    uid: "builtin-youtube",
    name: "YouTube",
    url: "https://www.youtube.com",
    builtIn: true,
    icon: YouTubeIcon,
  },
  "builtin-chatgpt": {
    uid: "builtin-chatgpt",
    name: "ChatGPT",
    url: "https://chatgpt.com",
    builtIn: true,
    icon: ChatGptIcon,
  },
  "builtin-anthropic": {
    uid: "builtin-anthropic",
    name: "Anthropic",
    url: "https://www.anthropic.com",
    builtIn: true,
    icon: AnthropicIcon,
  },
  "builtin-grok": {
    uid: "builtin-grok",
    name: "Grok",
    url: "https://grok.com",
    builtIn: true,
    icon: GrokIcon,
  },
};

const DEFAULT_TEST_ORDER = [
  "builtin-google",
  "builtin-youtube",
  "builtin-apple",
  "builtin-github",
  "builtin-chatgpt",
  "builtin-anthropic",
  "builtin-grok",
];

const DEFAULT_TEST_LIST: IVergeTestItem[] = DEFAULT_TEST_ORDER.map(
  (uid) => DEFAULT_TEST_ITEMS[uid]
);

const DEFAULT_TEST_MAP = new Map(
  DEFAULT_TEST_LIST.map((item) => [`${item.name}::${item.url}`, item])
);

const TestPage = () => {
  const { t } = useTranslation();
  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );
  const { verge, mutateVerge, patchVerge } = useVerge();

  // test list
  const testList = useMemo(
    () =>
      verge?.test_list
        ? normalizeTestList(verge.test_list, DEFAULT_TEST_MAP).normalized
        : DEFAULT_TEST_LIST,
    [verge?.test_list]
  );

  const onTestListItemChange = (
    uid: string,
    patch?: Partial<IVergeTestItem>
  ) => {
    if (patch) {
      const newList = testList.map((x) => {
        if (x.uid === uid) {
          return { ...x, ...patch };
        }
        return x;
      });
      mutateVerge({ ...verge, test_list: newList }, false);
    } else {
      mutateVerge();
    }
  };

  const onDeleteTestListItem = (uid: string) => {
    const target = testList.find((x) => x.uid === uid);

    if (target?.builtIn) {
      return;
    }

    const newList = testList.filter((x) => x.uid !== uid);
    patchVerge({ test_list: newList });
    mutateVerge({ ...verge, test_list: newList }, false);
  };

  const reorder = (list: any[], startIndex: number, endIndex: number) => {
    const result = Array.from(list);
    const [removed] = result.splice(startIndex, 1);
    result.splice(endIndex, 0, removed);
    return result;
  };

  const onDragEnd = async (event: DragEndEvent) => {
    const { active, over } = event;
    if (over) {
      if (active.id !== over.id) {
        let old_index = testList.findIndex((x) => x.uid === active.id);
        let new_index = testList.findIndex((x) => x.uid === over.id);
        if (old_index < 0 || new_index < 0) {
          return;
        }
        let newList = reorder(testList, old_index, new_index);
        await mutateVerge({ ...verge, test_list: newList }, false);
        await patchVerge({ test_list: newList });
      }
    }
  };

  useEffect(() => {
    if (!verge) return;

    if (!verge.test_list) {
      patchVerge({ test_list: DEFAULT_TEST_LIST }).catch(() => undefined);
      mutateVerge({ ...verge, test_list: DEFAULT_TEST_LIST }, false);
      return;
    }

    const { normalized, changed } = normalizeTestList(
      verge.test_list,
      DEFAULT_TEST_MAP
    );

    if (changed) {
      patchVerge({ test_list: normalized }).catch(() => undefined);
      mutateVerge({ ...verge, test_list: normalized }, false);
    }
  }, [verge]);

  const viewerRef = useRef<TestViewerRef>(null);
  const [testing, setTesting] = useState(false);
  const [pending, setPending] = useState<Set<string>>(() => new Set());
  const [resetToken, setResetToken] = useState(0);
  const [autoResults, setAutoResults] = useState<SiteTestResults>(() =>
    getFreshSiteTestResults()
  );
  const [lastResultAt, setLastResultAt] = useState(() =>
    Object.values(getFreshSiteTestResults()).reduce(
      (latest, entry) => Math.max(latest, entry.at),
      0
    )
  );

  // 站点检测结果由全局事件写入模块缓存，切页回来直接读缓存
  useEffect(
    () =>
      subscribeSiteTestResults((results) => {
        setAutoResults({ ...results });
        const latest = Object.values(results).reduce(
          (value, entry) => Math.max(value, entry.at),
          0
        );
        if (latest > 0) {
          setLastResultAt((prev) => Math.max(prev, latest));
        }
      }),
    []
  );

  // 结果到齐后清掉对应卡片的等待状态
  useEffect(() => {
    if (pending.size === 0) return;
    setPending((prev) => {
      let changed = false;
      const next = new Set(prev);
      for (const url of Object.keys(autoResults)) {
        if (next.delete(url)) changed = true;
      }
      return changed ? next : prev;
    });
  }, [autoResults]);

  // 最后一次测试结果超过 2 分钟后，整页一起回到初始状态
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (testing) return;
      if (
        lastResultAt > 0 &&
        Date.now() - lastResultAt >= SITE_TEST_RESULT_TTL_MS
      ) {
        clearSiteTestResults();
        setLastResultAt(0);
        setPending(new Set());
        setResetToken((token) => token + 1);
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [testing, lastResultAt]);

  return (
    <BasePage
      title={t("Test")}
      header={
        <Box sx={{ display: "flex", alignItems: "center", gap: 1 }}>
          <Button
            variant="contained"
            size="small"
            disabled={testing}
            onClick={async () => {
              if (testing) return;
              const urls = Array.from(
                new Set(testList.map((item) => item.url))
              );
              clearSiteTestResults();
              setLastResultAt(0);
              setPending(new Set(urls));
              setTesting(true);
              try {
                await Promise.race([
                  triggerAutoSiteTest(),
                  new Promise<never>((_, reject) => {
                    window.setTimeout(
                      () => reject(new Error("site test timed out")),
                      60000
                    );
                  }),
                ]);
              } catch (err: any) {
                Notice.error(formatNoticeMessage(err));
              } finally {
                setTesting(false);
                setPending(new Set());
              }
            }}
          >
            {testing ? t("Testing") : t("Test All")}
          </Button>
          <Button
            variant="contained"
            size="small"
            onClick={() => viewerRef.current?.create()}
          >
            {t("New")}
          </Button>
        </Box>
      }
    >
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={onDragEnd}
      >
        <Box sx={{ mb: 4.5 }}>
          <Grid container spacing={{ xs: 1, lg: 1 }}>
            <SortableContext
              items={testList.map((x) => {
                return x.uid;
              })}
            >
              {testList.map((item) => {
                const editable = !item.builtIn;

                return (
                  <Grid item xs={6} sm={4} md={3} lg={2} key={item.uid}>
                    <TestItem
                      id={item.uid}
                      itemData={item}
                      autoResult={autoResults[item.url]?.result}
                      pending={pending.has(item.url)}
                      resetToken={resetToken}
                      onTested={() => setLastResultAt(Date.now())}
                      editable={editable}
                      onEdit={() => {
                        if (editable) {
                          viewerRef.current?.edit(item);
                        }
                      }}
                      onDelete={onDeleteTestListItem}
                    />
                  </Grid>
                );
              })}
            </SortableContext>
          </Grid>
        </Box>
      </DndContext>
      <TestViewer ref={viewerRef} onChange={onTestListItemChange} />
    </BasePage>
  );
};

export default TestPage;
