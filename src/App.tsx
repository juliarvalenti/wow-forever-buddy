import { Badge } from "@/components/ui/warcraftcn/badge";
import { Button } from "@/components/ui/warcraftcn/button";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/warcraftcn/tabs";

const SECTIONS = [
  {
    id: "addons",
    label: "Addons",
    title: "Addons",
    blurb: "Browse, install, and update addons for your Forever client.",
  },
  {
    id: "macros",
    label: "Macros",
    title: "Macros",
    blurb: "Write and organize macros outside the game.",
  },
  {
    id: "settings",
    label: "Settings",
    title: "Settings",
    blurb: "Point the buddy at your World of Warcraft folder.",
  },
] as const;

export default function App() {
  return (
    <main className="min-h-screen bg-[radial-gradient(ellipse_at_top,#2a2116_0%,#0c0a08_70%)] px-6 py-8 text-amber-50">
      <header className="mx-auto mb-8 flex max-w-5xl items-center justify-between gap-4">
        <div>
          <h1 className="fantasy text-3xl font-bold text-amber-200 [text-shadow:0_0_12px_rgba(251,191,36,0.35)]">
            WoW Forever Buddy
          </h1>
          <p className="fantasy text-sm text-amber-100/60">
            Your companion for World of Warcraft: Forever
          </p>
        </div>
        <Badge>v0.1.0</Badge>
      </header>

      <Tabs defaultValue="addons" className="mx-auto max-w-5xl">
        <TabsList>
          {SECTIONS.map((s) => (
            <TabsTrigger key={s.id} value={s.id}>
              {s.label}
            </TabsTrigger>
          ))}
        </TabsList>

        {SECTIONS.map((s) => (
          <TabsContent key={s.id} value={s.id}>
            <div className="flex flex-col items-start gap-4">
              <h2 className="fantasy text-2xl font-bold text-amber-200">
                {s.title}
              </h2>
              <p className="fantasy text-amber-50/80">{s.blurb}</p>
              <p className="text-sm text-amber-100/60">Nothing here yet.</p>
              <Button>Coming soon</Button>
            </div>
          </TabsContent>
        ))}
      </Tabs>
    </main>
  );
}
