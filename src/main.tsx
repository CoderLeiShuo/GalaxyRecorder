import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './styles.css';
import './mobile-tech.css';
import './glacier-a.css';
import './media-ui.css';
import './device-ui.css';

document.addEventListener('contextmenu',event=>event.preventDefault());
document.addEventListener('dragstart',event=>event.preventDefault());

ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode><App /></React.StrictMode>);
