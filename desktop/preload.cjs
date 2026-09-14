const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('nexaSecrets', {
  save: values => ipcRenderer.invoke('nexa:save-secrets', values),
  chooseProjectDirectory: () => ipcRenderer.invoke('nexa:choose-project-directory')
});
