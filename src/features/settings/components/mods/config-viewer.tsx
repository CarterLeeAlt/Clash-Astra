import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { Chip } from "@mui/material";
import { getRuntimeYaml } from "@/services/cmds";
import { BaseDialog, DialogRef } from "@/components/base";
import { editorFontFamily, loadEditorFont } from "@/shared/fonts";
import { textSizePx } from "@/shared/typography";
import { editor } from "monaco-editor/esm/vs/editor/editor.api";

import "monaco-editor/esm/vs/basic-languages/javascript/javascript.contribution.js";
import "monaco-editor/esm/vs/basic-languages/yaml/yaml.contribution.js";
import "monaco-editor/esm/vs/editor/contrib/folding/browser/folding.js";
import "monaco-editor/esm/vs/editor/contrib/find/browser/findController.js";

export const ConfigViewer = forwardRef<DialogRef>((_props, ref) => {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const editorRef = useRef<any>();
  const instanceRef = useRef<editor.IStandaloneCodeEditor | null>(null);

  useEffect(() => {
    return () => {
      if (instanceRef.current) {
        instanceRef.current.dispose();
        instanceRef.current = null;
      }
    };
  }, []);

  useImperativeHandle(ref, () => ({
    open: () => {
      setOpen(true);

      Promise.all([getRuntimeYaml(), loadEditorFont()]).then(([data]) => {
        const dom = editorRef.current;

        if (!dom) return;
        if (instanceRef.current) instanceRef.current.dispose();

        instanceRef.current = editor.create(editorRef.current, {
          value: data ?? "# Error\n",
          language: "yaml",
          theme: "vs",
          minimap: { enabled: false },
          fontSize: textSizePx.code,
          fontFamily: editorFontFamily,
          fontWeight: "300",
          readOnly: true,
        });

        instanceRef.current.focus();
      });
    },
    close: () => setOpen(false),
  }));

  return (
    <BaseDialog
      open={open}
      title={
        <>
          {t("Runtime Config")} <Chip label={t("ReadOnly")} size="small" />
        </>
      }
      maxWidth="xl"
      fullWidth
      contentSx={{ width: "95%", pb: 1, userSelect: "text" }}
      cancelBtn={t("Back")}
      disableOk
      onClose={() => setOpen(false)}
      onCancel={() => setOpen(false)}
    >
      <div style={{ width: "100%", height: "500px" }} ref={editorRef} />
    </BaseDialog>
  );
});
