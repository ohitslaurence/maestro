---
paths: "**/*.{ts,tsx,js,jsx}"
---

## React Hooks

When reviewing or writing `useEffect`, `useState` for derived values, or state synchronization patterns:

- Invoke the `react-useeffect` skill
- Prefer derived values over state + effect patterns
- Use `useMemo` for expensive calculations, not `useEffect`
- Check for missing cleanup functions in effects with subscriptions
