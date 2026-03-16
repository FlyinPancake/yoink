import { MonitorIcon, MoonIcon, SunIcon } from "lucide-react";
import { useTheme, type Theme } from "@/hooks/use-theme";
import { cn } from "@/lib/utils";

const options: Array<{
  value: Theme;
  icon: React.ComponentType<{ className?: string }>;
  label: string;
}> = [
  { value: "light", icon: SunIcon, label: "Light" },
  { value: "dark", icon: MoonIcon, label: "Dark" },
  { value: "system", icon: MonitorIcon, label: "System" },
];

export function ThemeSelector() {
  const { theme, setTheme } = useTheme();

  return (
    <div
      className={cn(
        "group/theme flex items-center rounded-full border border-border bg-muted/60 p-0.5",
        "transition-all duration-300 ease-in-out",
      )}
    >
      {options.map(({ value, icon: Icon, label }) => {
        const isSelected = theme === value;

        return (
          <button
            key={value}
            type="button"
            onClick={() => setTheme(value)}
            aria-label={label}
            className={cn(
              // Fixed-size button so layout never shifts on selection change
              "flex size-7 items-center justify-center rounded-full",
              "transition-all duration-300 ease-in-out",
              // Selected: always visible and highlighted
              isSelected && "bg-background text-foreground shadow-sm",
              // Not selected: hidden by default, revealed on group hover
              !isSelected && [
                "text-muted-foreground hover:text-foreground",
                // Collapse: zero width + invisible, with grace period
                "max-w-0 overflow-hidden opacity-0 delay-200",
                // Expand on group hover (instant, no delay)
                "group-hover/theme:max-w-7 group-hover/theme:opacity-100 group-hover/theme:delay-0",
              ],
            )}
          >
            <Icon className="size-3.5 shrink-0" />
          </button>
        );
      })}
    </div>
  );
}
