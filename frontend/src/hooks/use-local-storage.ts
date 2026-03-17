import { useEffect, useState } from "react";

/**
 * A hook that persists state in localStorage.
 * @param key The localStorage key to use
 * @param defaultValue The default value if no value exists in localStorage
 * @returns A stateful value and a function to update it
 */
export function useLocalStorage<T extends string>(
  key: string,
  defaultValue: T,
): [T, (value: T) => void] {
  const [state, setState] = useState<T>(() => {
    const saved = localStorage.getItem(key);
    return (saved as T) ?? defaultValue;
  });

  useEffect(() => {
    localStorage.setItem(key, state);
  }, [key, state]);

  return [state, setState];
}
