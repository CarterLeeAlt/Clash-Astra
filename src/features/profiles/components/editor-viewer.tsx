import { useEffect, useRef } from "react";
import { useLockFn } from "ahooks";
import { useTranslation } from "react-i18next";
import {
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
} from "@mui/material";
import { readProfileFile, saveProfileFile } from "@/services/cmds";
import { formatNoticeMessage, Notice } from "@/components/base";
import { editorFontFamily, loadEditorFont } from "@/shared/fonts";
import { textSizePx } from "@/shared/typography";

import "monaco-editor/esm/vs/basic-languages/javascript/javascript.contribution.js";
import "monaco-editor/esm/vs/basic-languages/yaml/yaml.contribution.js";
import "monaco-editor/esm/vs/editor/contrib/folding/browser/folding.js";
import "monaco-editor/esm/vs/editor/contrib/find/browser/findController.js";
import { editor } from "monaco-editor/esm/vs/editor/editor.api";

interface Props {
  uid: string;
  open: boolean;
  mode: "yaml" | "javascript";
  onClose: () => void;
  onChange?: () => void;
}

export const EditorViewer = (props: Props) => {
  const { uid, open, mode, onClose, onChange } = props;

  const { t } = useTranslation();
  const editorRef = useRef<any>();
  const instanceRef = useRef<editor.IStandaloneCodeEditor | null>(null);

  useEffect(() => {
    if (!open) return;
    let disposed = false;

    Promise.all([readProfileFile(uid), loadEditorFont()])
      .then(([data]) => {
        if (disposed) return;
        const dom = editorRef.current;

        if (!dom) return;
        if (instanceRef.current) instanceRef.current.dispose();

        instanceRef.current = editor.create(editorRef.current, {
          value: data,
          language: mode,
          theme: "vs",
          minimap: { enabled: false },
          fontSize: textSizePx.code,
          fontFamily: editorFontFamily,
          fontWeight: "400",
        });

        instanceRef.current.focus();
      })
      .catch((error) => {
        if (!disposed) Notice.error(formatNoticeMessage(error));
      });

    return () => {
      disposed = true;
      if (instanceRef.current) {
        instanceRef.current.dispose();
        instanceRef.current = null;
      }
    };
  }, [open, uid, mode]);

  const onSave = useLockFn(async () => {
    const value = instanceRef.current?.getValue();

    if (value == null) return;

    try {
      await saveProfileFile(uid, value);
      onChange?.();
      onClose();
    } catch (err: any) {
      Notice.error(formatNoticeMessage(err));
    }
  });

  return (
    <Dialog open={open} onClose={onClose} maxWidth="xl" fullWidth>
      <DialogTitle>{t("Edit File")}</DialogTitle>

      <DialogContent sx={{ width: "95%", pb: 1, userSelect: "text" }}>
        <div style={{ width: "100%", height: "500px" }} ref={editorRef} />
      </DialogContent>

      <DialogActions>
        <Button onClick={onClose} variant="outlined">
          {t("Cancel")}
        </Button>
        <Button onClick={onSave} variant="contained">
          {t("Save")}
        </Button>
      </DialogActions>
    </Dialog>
  );
};
