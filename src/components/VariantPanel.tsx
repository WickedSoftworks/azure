import { cn } from "cn";
import type { Preset } from "@/lib/bindings";

interface Props {
  presets: Preset[];
  /** Executable (lowercased) to the preset id that game will activate. */
  preferred: Record<string, string>;
  activeId: string;
  live: boolean;
  onAddVariant: (exe: string, name: string) => void;
  onUse: (id: string) => void;
  onSelect: (id: string) => void;
}

interface Game {
  exe: string;
  label: string;
  variants: Preset[];
}

/** `D:\games\cs2.exe` becomes `CS2`. */
function labelFor(exe: string): string {
  const i = Math.max(exe.lastIndexOf("\\"), exe.lastIndexOf("/"));
  const base = i >= 0 ? exe.slice(i + 1) : exe;
  return base.replace(/\.exe$/i, "").toUpperCase() || "GAME";
}

/**
 * Groups presets by the executable they are bound to.
 *
 * Two presets on the same executable are two looks for one game, not two
 * games. The desktop preset is bound to nothing and is not a game, so it
 * does not appear here.
 */
function gamesOf(presets: Preset[]): Game[] {
  const games = new Map<string, Game>();

  for (const preset of presets) {
    if (!preset.exe) continue;
    const key = preset.exe.toLowerCase();
    const existing = games.get(key);
    if (existing) {
      existing.variants.push(preset);
    } else {
      games.set(key, { exe: preset.exe, label: labelFor(preset.exe), variants: [preset] });
    }
  }

  return [...games.values()].sort((a, b) => a.label.localeCompare(b.label));
}

/**
 * Which variant a game will activate: the one chosen, or the first, which
 * is how a game with a single preset has always behaved.
 */
function chosenId(game: Game, preferred: Record<string, string>): string {
  const chosen = preferred[game.exe.toLowerCase()];
  if (chosen && game.variants.some((v) => v.id === chosen)) return chosen;
  return game.variants[0].id;
}

/**
 * Several looks for one game, and which one it gets.
 *
 * The watcher activates exactly one preset per game. Before this panel a
 * second preset bound to the same executable could be created and could
 * never fire, with no symptom except that nothing happened — so the
 * choice is shown here rather than inferred.
 */
export function VariantPanel({
  presets,
  preferred,
  activeId,
  live,
  onAddVariant,
  onUse,
  onSelect,
}: Props) {
  const games = gamesOf(presets);

  if (games.length === 0) {
    return (
      <section className="ng-rule-t flex flex-col">
        <div className="ng-rule-b flex items-baseline gap-x-3 px-4 py-1.5">
          <span className="ng-label">PER-GAME LOOKS</span>
        </div>
        <p className="px-4 py-2 text-dim">
          Nothing is bound to a game yet. Scan your library, capture a window or add a
          game, then you can keep more than one look for it.
        </p>
      </section>
    );
  }

  return (
    <section className="ng-rule-t flex flex-col">
      <div className="ng-rule-b flex flex-wrap items-baseline gap-x-3 gap-y-1 px-4 py-1.5">
        <span className="ng-label">PER-GAME LOOKS</span>
        <span className="text-dim">
          a game activates one look — keep several and choose which
        </span>
      </div>

      {games.map((game) => {
        const chosen = chosenId(game, preferred);
        const next = `${game.label} ${game.variants.length + 1}`;

        return (
          <div key={game.exe.toLowerCase()} className="ng-rule-b px-4 py-2">
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
              <span className="ng-label text-text">{game.label}</span>
              <span className="text-dim">
                {game.variants.length === 1
                  ? "one look"
                  : `${game.variants.length} looks`}
              </span>
              <button
                type="button"
                onClick={() => onAddVariant(game.exe, next)}
                disabled={!live}
                className="ml-auto ng-label text-signal hover:text-bright disabled:opacity-40"
              >
                + ADD A LOOK
              </button>
            </div>

            <div className="mt-1 flex flex-col">
              {game.variants.map((variant) => {
                const isChosen = variant.id === chosen;
                const isActive = variant.id === activeId;

                return (
                  <div
                    key={variant.id}
                    className="grid grid-cols-[auto_1fr_auto] items-baseline gap-x-3 py-0.5"
                  >
                    {/* The mark says what this game will do, which is a
                        different question from what is on screen now. */}
                    <span
                      className={cn("ng-label", isChosen ? "text-signal" : "text-dim")}
                      title={isChosen ? "this game activates this look" : undefined}
                    >
                      {isChosen ? "◉" : "○"}
                    </span>

                    <button
                      type="button"
                      onClick={() => onSelect(variant.id)}
                      disabled={!live}
                      className={cn(
                        "text-left disabled:opacity-40",
                        isActive ? "text-bright" : "text-text hover:text-bright",
                      )}
                    >
                      {variant.name}
                      {isActive && <span className="ng-label text-signal"> · ON SCREEN</span>}
                    </button>

                    {!isChosen && (
                      <button
                        type="button"
                        onClick={() => onUse(variant.id)}
                        disabled={!live}
                        className="ng-label text-dim hover:text-text disabled:opacity-40"
                      >
                        USE FOR THIS GAME
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </section>
  );
}
