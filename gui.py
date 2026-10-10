"""PZ Mod Manager - Tkinter front end for pzmm."""
import os, queue, subprocess, sys, threading, tkinter as tk
from tkinter import messagebox, ttk

import pzmm


class Log:
    """stdout/stderr -> queue, drained on the Tk thread."""
    def __init__(self, q):
        self.q = q

    def write(self, text):
        self.q.put(text)

    def flush(self):
        pass


def main():
    root = tk.Tk()
    root.title("PZ Mod Manager")
    root.geometry("960x620")
    q = queue.Queue()
    sys.stdout = sys.stderr = Log(q)
    b41 = tk.BooleanVar(value=pzmm.read_enabled()[2])
    busy = tk.BooleanVar(value=False)

    top = ttk.Frame(root, padding=6)
    top.pack(fill="x")
    ttk.Label(top, text="Workshop ID/URL or GitHub owner/repo[@ref]:").pack(side="left")
    src = ttk.Entry(top)
    src.pack(side="left", fill="x", expand=True, padx=6)

    cols = ("on", "id", "name", "source")
    tree = ttk.Treeview(root, columns=cols, show="headings")
    for c, w, label in zip(cols, (40, 220, 320, 260), ("On", "Mod ID", "Name", "Source")):
        tree.heading(c, text=label)
        tree.column(c, width=w, stretch=c != "on", anchor="center" if c == "on" else "w")
    tree.pack(fill="both", expand=True, padx=6)

    bar = ttk.Frame(root, padding=6)
    bar.pack(fill="x")
    log = tk.Text(root, height=8, state="disabled", wrap="word")
    log.pack(fill="x", padx=6, pady=(0, 6))

    def refresh():
        mods, (enabled, *_) = pzmm.installed(), pzmm.read_enabled()
        tree.delete(*tree.get_children())
        for i in dict.fromkeys(enabled):  # load order first
            name, srce, _ = mods.get(i, ("(missing)", "", None))
            tree.insert("", "end", iid=i, values=("✔", i, name, srce))
        for i, (name, srce, _) in sorted(mods.items(), key=lambda kv: kv[0].lower()):
            if i not in enabled:
                tree.insert("", "end", iid=i, values=("", i, name, srce))

    def selected():
        return list(tree.selection())

    def save(ids):
        pzmm.write_enabled(ids, b41.get())
        sel = selected()
        refresh()
        tree.selection_set([i for i in sel if tree.exists(i)])

    def toggle(_=None):
        enabled, sel = pzmm.read_enabled()[0], selected()
        if sel and all(i in enabled for i in sel):
            save([i for i in enabled if i not in sel])
        else:
            save(enabled + sel)

    def move(step):
        enabled = pzmm.read_enabled()[0]
        sel = [i for i in selected() if i in enabled]
        for i in sel if step < 0 else reversed(sel):
            j = enabled.index(i)
            if 0 <= j + step < len(enabled) and enabled[j + step] not in sel:
                enabled[j], enabled[j + step] = enabled[j + step], enabled[j]
        save(enabled)

    def remove():
        sel = selected()
        if sel and messagebox.askyesno("Remove", f"Delete {len(sel)} mod(s) from Zomboid/mods?"):
            pzmm.remove(sel)
            save([i for i in pzmm.read_enabled()[0] if i not in sel])

    def background(job, enable_result=False):
        if busy.get():
            return
        busy.set(True)

        def work():
            try:
                ids = job()
                if enable_result and ids:
                    q.put(lambda: save(pzmm.read_enabled()[0] + ids))
                print("done.")
            except Exception as e:
                print(f"error: {e}")
            q.put(refresh)
            q.put(lambda: busy.set(False))
        threading.Thread(target=work, daemon=True).start()

    def install(_=None):
        sources = src.get().split()
        if sources:
            src.delete(0, "end")
            background(lambda: pzmm.add(sources), enable_result=True)

    def open_folder():
        pzmm.MODS.mkdir(parents=True, exist_ok=True)
        os.startfile(pzmm.MODS) if os.name == "nt" else subprocess.Popen(["xdg-open", pzmm.MODS])

    def drain():
        while not q.empty():
            item = q.get()
            if callable(item):
                item()
            else:
                log.configure(state="normal")
                log.insert("end", item)
                log.see("end")
                log.configure(state="disabled")
        root.after(100, drain)

    ttk.Button(top, text="Install", command=install).pack(side="left")
    src.bind("<Return>", install)
    tree.bind("<Double-1>", toggle)
    for text, cmd in (("Enable / Disable", toggle), ("Move Up", lambda: move(-1)), ("Move Down", lambda: move(1)),
                      ("Remove", remove), ("Update All", lambda: background(pzmm.update)),
                      ("Refresh", refresh), ("Open Mods Folder", open_folder)):
        ttk.Button(bar, text=text, command=cmd).pack(side="left", padx=(0, 4))
    ttk.Checkbutton(bar, text="Build 41 format", variable=b41).pack(side="right")

    print(f"Zomboid folder: {pzmm.HOME}\nSteam Workshop folder: {pzmm.steam_workshop() or 'not found'}\n")
    refresh()
    drain()
    root.mainloop()


if __name__ == "__main__":
    main()
