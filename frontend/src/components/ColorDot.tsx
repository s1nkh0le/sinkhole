interface ColorBarProps {
  color?: string | null;
}

/**
 * Vertical color bar indicator. Used instead of a circle
 * to avoid confusion with StatusDot (health status circles).
 */
export function ColorBar({ color }: ColorBarProps) {
  if (!color) return null;
  return (
    <span
      className="color-bar"
      style={{
        display: 'inline-block',
        width: 3,
        height: '1.2em',
        borderRadius: 2,
        backgroundColor: color,
        flexShrink: 0,
      }}
    />
  );
}
