

interface OfficeHubLogoProps {
  className?: string;
  size?: number;
}

export function OfficeHubLogo({ className = '', size = 32 }: OfficeHubLogoProps) {
  return (
    <div className={`relative flex items-center justify-center ${className}`} style={{ width: size, height: size }}>
      <svg
        width={size}
        height={size}
        viewBox="0 0 100 100"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className="transition-transform duration-300 hover:scale-105"
      >
        {/* Top Left - Word Blue */}
        <path
          d="M10 45C10 25.67 25.67 10 45 10H48V48H10V45Z"
          fill="#2B579A"
          className="hover:opacity-90 transition-opacity"
        />
        {/* Top Right - Excel Green */}
        <path
          d="M52 10H80C91.0457 10 100 18.9543 100 30V48H52V10Z"
          fill="#217346"
          className="hover:opacity-90 transition-opacity"
        />
        {/* Bottom Left - PowerPoint Orange */}
        <path
          d="M10 52H48V90H30C18.9543 90 10 81.0457 10 70V52Z"
          fill="#D24726"
          className="hover:opacity-90 transition-opacity"
        />
        {/* Bottom Right - Yellow */}
        <path
          d="M52 52H100V70C100 81.0457 91.0457 90 80 90H52V52Z"
          fill="#FFB900"
          className="hover:opacity-90 transition-opacity"
        />
        
        {/* Center Overlay to create a connection/origami feel */}
        <rect x="40" y="40" width="20" height="20" rx="4" fill="white" fillOpacity="0.2" className="backdrop-blur-sm" />
      </svg>
    </div>
  );
}
