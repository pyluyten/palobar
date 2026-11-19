export const lockCss = `
window.lock-window {
  background-color: #131316;
  color: #f6f5f4;
  font-family: "Cantarell", "Inter", sans-serif;
}

.lock-bg {
  transition: filter 350ms ease, opacity 350ms ease;
}

.lock-bg.prompt {
  filter: blur(18px) brightness(0.55);
}

.lock-time {
  font-size: 86px;
  font-weight: 700;
  color: #ffffff;
  text-shadow: 0 2px 16px rgba(0, 0, 0, 0.45);
}

.lock-date {
  font-size: 22px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.88);
  text-shadow: 0 1px 10px rgba(0, 0, 0, 0.45);
}

.lock-avatar {
  border-radius: 999px;
  background-color: rgba(255, 255, 255, 0.14);
  border: 2px solid rgba(255, 255, 255, 0.25);
}

.lock-username {
  font-size: 20px;
  font-weight: 700;
  color: #ffffff;
}

.lock-entry {
  background-color: rgba(255, 255, 255, 0.16);
  border: 1px solid rgba(255, 255, 255, 0.24);
  border-radius: 999px;
  padding: 8px 16px;
  color: #ffffff;
  font-size: 14px;
}

.lock-entry:focus-within {
  border-color: #3584e4;
  background-color: rgba(255, 255, 255, 0.22);
}

.lock-error {
  color: #ff7b63;
  font-size: 13px;
  font-weight: 600;
}
`;
