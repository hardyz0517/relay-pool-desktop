import { useEffect, useState, type FormEvent } from "react";
import { KeyRound, Pencil, Plus, Trash2 } from "lucide-react";
import { Button, ConfirmDialog, Dialog, SectionCard, useToast } from "@/components/ui";
import {
  deleteCommonLoginProfile,
  listCommonLoginProfiles,
  upsertCommonLoginProfile,
} from "@/lib/api/settings";
import { readError } from "@/lib/errors";
import type { CommonLoginProfile } from "@/lib/types/settings";

type ProfileDraft = {
  id: string | null;
  email: string;
  password: string;
};

const emptyDraft: ProfileDraft = { id: null, email: "", password: "" };

export function CommonLoginProfilesSettings() {
  const toast = useToast();
  const [profiles, setProfiles] = useState<CommonLoginProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [draft, setDraft] = useState<ProfileDraft | null>(null);
  const [deleting, setDeleting] = useState<CommonLoginProfile | null>(null);

  useEffect(() => {
    let alive = true;
    void listCommonLoginProfiles()
      .then((items) => {
        if (alive) setProfiles(items);
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
    if (!draft?.email.trim() || (!draft.id && !draft.password.trim())) return;
    setSaving(true);
    try {
      const saved = await upsertCommonLoginProfile({
        id: draft.id,
        email: draft.email.trim(),
        password: draft.password.trim() ? draft.password : null,
      });
      setProfiles((current) => {
        const index = current.findIndex((profile) => profile.id === saved.id);
        if (index < 0) return [...current, saved];
        return current.map((profile) => (profile.id === saved.id ? saved : profile));
      });
      setDraft(null);
      toast.success(draft.id ? "常用登录信息已更新" : "常用登录信息已添加");
    } catch (error) {
      toast.error("保存常用登录信息失败", readError(error));
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!deleting) return;
    setSaving(true);
    try {
      await deleteCommonLoginProfile(deleting.id);
      setProfiles((current) => current.filter((profile) => profile.id !== deleting.id));
      setDeleting(null);
      toast.success("常用登录信息已删除");
    } catch (error) {
      toast.error("删除常用登录信息失败", readError(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <SectionCard
        contentClassName="p-0"
        title="常用登录信息"
        action={
          <Button
            className="h-7 px-2.5 text-xs"
            disabled={loading || saving}
            type="button"
            variant="outline"
            onClick={() => setDraft(emptyDraft)}
          >
            <Plus className="h-3.5 w-3.5" />
            添加
          </Button>
        }
      >
        {loading ? (
          <div className="px-3 py-4 text-sm text-muted-foreground">读取中</div>
        ) : profiles.length === 0 ? (
          <div className="px-3 py-4 text-sm text-muted-foreground">
            暂无常用登录信息
          </div>
        ) : (
          profiles.map((profile) => (
            <div
              key={profile.id}
              className="grid min-h-12 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-border px-3 py-2 last:border-b-0"
            >
              <div className="min-w-0">
                <div className="truncate text-sm font-medium text-foreground">{profile.email}</div>
                <div className="mt-0.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                  <KeyRound className="h-3.5 w-3.5" />
                  {profile.passwordPresent ? profile.passwordMasked : "未设置密码"}
                </div>
              </div>
              <div className="flex items-center gap-1">
                <Button
                  aria-label={`编辑 ${profile.email}`}
                  className="h-7 w-7 px-0"
                  disabled={saving}
                  title="编辑"
                  type="button"
                  variant="ghost"
                  onClick={() => setDraft({ id: profile.id, email: profile.email, password: "" })}
                >
                  <Pencil className="h-3.5 w-3.5" />
                </Button>
                <Button
                  aria-label={`删除 ${profile.email}`}
                  className="h-7 w-7 px-0 text-danger-foreground"
                  disabled={saving}
                  title="删除"
                  type="button"
                  variant="ghost"
                  onClick={() => setDeleting(profile)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            </div>
          ))
        )}
      </SectionCard>

      <Dialog
        className="max-w-md"
        open={draft !== null}
        title={draft?.id ? "编辑常用登录信息" : "添加常用登录信息"}
        description={draft?.id ? "密码留空将保留当前密码" : undefined}
        onClose={() => !saving && setDraft(null)}
        footer={
          <div className="flex justify-end gap-2">
            <Button disabled={saving} type="button" variant="outline" onClick={() => setDraft(null)}>
              取消
            </Button>
            <Button
              disabled={saving || !draft?.email.trim() || (!draft.id && !draft.password.trim())}
              form="common-login-profile-form"
              type="submit"
            >
              {saving ? "保存中" : "保存"}
            </Button>
          </div>
        }
      >
        <form id="common-login-profile-form" className="grid gap-4 p-5" onSubmit={handleSave}>
          <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
            邮箱
            <input
              autoComplete="email"
              className={inputClassName}
              required
              type="email"
              value={draft?.email ?? ""}
              onChange={(event) => setDraft((current) => current && { ...current, email: event.target.value })}
            />
          </label>
          <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
            密码
            <input
              autoComplete="new-password"
              className={inputClassName}
              placeholder={draft?.id ? "留空保留当前密码" : "输入常用密码"}
              required={!draft?.id}
              type="password"
              value={draft?.password ?? ""}
              onChange={(event) => setDraft((current) => current && { ...current, password: event.target.value })}
            />
          </label>
        </form>
      </Dialog>

      <ConfirmDialog
        cancelLabel="取消"
        confirmLabel="删除"
        confirming={saving}
        description="删除后，供应商表单将不能再使用这组邮箱和密码快速填充。"
        open={deleting !== null}
        title="删除常用登录信息？"
        onCancel={() => !saving && setDeleting(null)}
        onConfirm={() => void handleDelete()}
      />
    </>
  );
}

const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-control px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-control focus:ring-2 focus:ring-ring/30";
