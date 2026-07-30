import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { AtSign, KeyRound, Pencil, Plus, Trash2 } from "lucide-react";
import { Button, ConfirmDialog, Dialog, IconButton, SectionCard, useToast } from "@/components/ui";
import {
  deleteCommonLoginEmail,
  deleteCommonLoginPassword,
  listCommonLoginOptions,
  upsertCommonLoginEmail,
  upsertCommonLoginPassword,
} from "@/lib/api/settings";
import { readError } from "@/lib/errors";
import type { CommonLoginOptions } from "@/lib/types/settings";

type OptionKind = "email" | "password";

type OptionDraft = {
  kind: OptionKind;
  id: string | null;
  value: string;
};

type DeleteTarget = {
  kind: OptionKind;
  id: string;
  label: string;
};

const emptyOptions: CommonLoginOptions = { emails: [], passwords: [] };

export function CommonLoginProfilesSettings() {
  const toast = useToast();
  const [options, setOptions] = useState<CommonLoginOptions>(emptyOptions);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [draft, setDraft] = useState<OptionDraft | null>(null);
  const [deleting, setDeleting] = useState<DeleteTarget | null>(null);

  useEffect(() => {
    let alive = true;
    void listCommonLoginOptions()
      .then((items) => {
        if (alive) setOptions(items);
      })
      .catch((error) => {
        if (alive) toast.error("读取常用登录信息失败", readError(error));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [toast]);

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!draft?.value.trim()) return;
    setSaving(true);
    try {
      if (draft.kind === "email") {
        const saved = await upsertCommonLoginEmail({
          id: draft.id,
          email: draft.value.trim(),
        });
        setOptions((current) => ({
          ...current,
          emails: replaceById(current.emails, saved),
        }));
        toast.success(draft.id ? "常用邮箱已更新" : "常用邮箱已添加");
      } else {
        const saved = await upsertCommonLoginPassword({
          id: draft.id,
          password: draft.value,
        });
        setOptions((current) => ({
          ...current,
          passwords: replaceById(current.passwords, saved),
        }));
        toast.success(draft.id ? "常用密码已更新" : "常用密码已添加");
      }
      setDraft(null);
    } catch (error) {
      toast.error(draft.kind === "email" ? "保存常用邮箱失败" : "保存常用密码失败", readError(error));
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!deleting) return;
    setSaving(true);
    try {
      if (deleting.kind === "email") {
        await deleteCommonLoginEmail(deleting.id);
        setOptions((current) => ({
          ...current,
          emails: current.emails.filter((item) => item.id !== deleting.id),
        }));
        toast.success("常用邮箱已删除");
      } else {
        await deleteCommonLoginPassword(deleting.id);
        setOptions((current) => ({
          ...current,
          passwords: current.passwords.filter((item) => item.id !== deleting.id),
        }));
        toast.success("常用密码已删除");
      }
      setDeleting(null);
    } catch (error) {
      toast.error(deleting.kind === "email" ? "删除常用邮箱失败" : "删除常用密码失败", readError(error));
    } finally {
      setSaving(false);
    }
  }

  const draftLabel = draft?.kind === "password" ? "密码" : "邮箱";

  return (
    <>
      <SectionCard contentClassName="p-0" title="常用登录信息">
        <div className="divide-y divide-border">
          <CommonLoginOptionGroup
            emptyLabel="暂无常用邮箱"
            items={options.emails.map((item) => ({
              id: item.id,
              label: item.email,
              icon: <AtSign className="h-3.5 w-3.5" />,
            }))}
            loading={loading}
            saving={saving}
            title="邮箱"
            onAdd={() => setDraft({ kind: "email", id: null, value: "" })}
            onDelete={(item) => setDeleting({ kind: "email", id: item.id, label: item.label })}
            onEdit={(item) => setDraft({ kind: "email", id: item.id, value: item.label })}
          />

          <CommonLoginOptionGroup
            emptyLabel="暂无常用密码"
            items={options.passwords.map((item) => ({
              id: item.id,
              label: item.passwordMasked,
              icon: <KeyRound className="h-3.5 w-3.5" />,
            }))}
            loading={loading}
            saving={saving}
            title="密码"
            onAdd={() => setDraft({ kind: "password", id: null, value: "" })}
            onDelete={(item) => setDeleting({ kind: "password", id: item.id, label: item.label })}
            onEdit={(item) => setDraft({ kind: "password", id: item.id, value: "" })}
          />
        </div>
      </SectionCard>

      <Dialog
        className="max-w-md"
        open={draft !== null}
        title={`${draft?.id ? "编辑" : "添加"}常用${draftLabel}`}
        description={draft?.kind === "password" && draft.id ? "输入新密码以替换当前密码" : undefined}
        onClose={() => !saving && setDraft(null)}
        footer={
          <div className="flex justify-end gap-2">
            <Button disabled={saving} type="button" variant="outline" onClick={() => setDraft(null)}>
              取消
            </Button>
            <Button
              disabled={saving || !draft?.value.trim()}
              form="common-login-option-form"
              type="submit"
            >
              {saving ? "保存中" : "保存"}
            </Button>
          </div>
        }
      >
        <form id="common-login-option-form" className="grid gap-4 p-5" onSubmit={handleSave}>
          <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
            {draftLabel}
            <input
              autoComplete={draft?.kind === "email" ? "email" : "new-password"}
              className={inputClassName}
              placeholder={draft?.kind === "password" ? "输入常用密码" : "name@example.com"}
              required
              type={draft?.kind === "password" ? "password" : "email"}
              value={draft?.value ?? ""}
              onChange={(event) => setDraft((current) => current && { ...current, value: event.target.value })}
            />
          </label>
        </form>
      </Dialog>

      <ConfirmDialog
        cancelLabel="取消"
        confirmLabel="删除"
        confirming={saving}
        description={`删除后，供应商表单将不能再快速填充这个${deleting?.kind === "password" ? "密码" : "邮箱"}。`}
        open={deleting !== null}
        title={`删除常用${deleting?.kind === "password" ? "密码" : "邮箱"}？`}
        onCancel={() => !saving && setDeleting(null)}
        onConfirm={() => void handleDelete()}
      />
    </>
  );
}

type CommonLoginOptionItem = {
  id: string;
  label: string;
  icon: ReactNode;
};

function CommonLoginOptionGroup({
  emptyLabel,
  items,
  loading,
  saving,
  title,
  onAdd,
  onDelete,
  onEdit,
}: {
  emptyLabel: string;
  items: CommonLoginOptionItem[];
  loading: boolean;
  saving: boolean;
  title: string;
  onAdd: () => void;
  onDelete: (item: CommonLoginOptionItem) => void;
  onEdit: (item: CommonLoginOptionItem) => void;
}) {
  return (
    <section aria-label={`常用${title}`}>
      <div className="flex min-h-10 items-center justify-between gap-3 px-3 py-2">
        <h3 className="text-xs font-semibold text-foreground">{title}</h3>
        <Button
          className="h-7 px-2.5 text-xs"
          disabled={loading || saving}
          type="button"
          variant="outline"
          onClick={onAdd}
        >
          <Plus className="h-3.5 w-3.5" />
          添加
        </Button>
      </div>
      <div className="border-t border-border">
        {loading ? (
          <div className="px-3 py-4 text-sm text-muted-foreground">读取中</div>
        ) : items.length === 0 ? (
          <div className="px-3 py-4 text-sm text-muted-foreground">{emptyLabel}</div>
        ) : (
          items.map((item) => (
            <div
              key={item.id}
              className="grid min-h-11 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-border px-3 py-2 last:border-b-0"
            >
              <div className="flex min-w-0 items-center gap-2 text-muted-foreground">
                {item.icon}
                <span className="truncate text-sm font-medium text-foreground">{item.label}</span>
              </div>
              <div className="flex items-center gap-1">
                <IconButton
                  className="h-7 w-7"
                  disabled={saving}
                  label={`编辑 ${item.label}`}
                  onClick={() => onEdit(item)}
                >
                  <Pencil className="h-3.5 w-3.5" />
                </IconButton>
                <IconButton
                  className="h-7 w-7 text-danger-foreground"
                  disabled={saving}
                  label={`删除 ${item.label}`}
                  onClick={() => onDelete(item)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </IconButton>
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function replaceById<T extends { id: string }>(items: T[], saved: T) {
  const existing = items.some((item) => item.id === saved.id);
  return existing
    ? items.map((item) => (item.id === saved.id ? saved : item))
    : [...items, saved];
}

const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-control px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-control focus:ring-2 focus:ring-ring/30";
