import "./Header.css";

export function Header({
  title,
  subtitle,
}: {
  title: string;
  subtitle?: string;
}) {
  return (
    <header className="app-header">
      <div className="app-header-titles">
        <h1 className="app-header-title">{title}</h1>
        {subtitle && <span className="app-header-subtitle">{subtitle}</span>}
      </div>
    </header>
  );
}
