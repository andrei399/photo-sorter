# Photo Sorter — Privacy Policy

Photo Sorter is made by Tudor Andrei-Cristian. It runs entirely on your computer.

- **No servers, no accounts, no telemetry.** We don't collect, receive or store any data about you or your photos. There are no analytics and no crash reports.
- **Your photos stay where you put them.** The app reads the folders you open and copies the photos you favourite into the session folders you choose. It never modifies or deletes your originals; removing a favourite moves only the app's own copy to your system's trash.
- **What the app stores:** your sessions, settings, the last opened folder and the names of connected cloud accounts, in one file in your system's application data folder. It never leaves your computer. Cloud sign-ins (access tokens, WebDAV passwords) are kept in your system keychain.
- **Network use:** at most once a day the app asks GitHub (github.com) whether a newer version exists, and downloads it if so. That request reveals your IP address and the app version to GitHub, under [GitHub's privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement). Nothing else is sent unless you connect a cloud service.
- **Cloud uploads (optional):** if you connect Google Drive, OneDrive, Dropbox or a WebDAV server (such as Nextcloud) and switch it on, your favourites are uploaded straight from your computer to that service, under its own privacy policy. Nothing passes through us. The app asks for the least access each service offers: on Google Drive it sees only the files it uploaded, and on OneDrive and Dropbox only its own `Apps/Photo Sorter` folder. Removing a favourite moves its cloud copy to that service's trash. Disconnecting deletes the sign-in from your keychain and leaves uploaded files in place.
- **Cloud folders:** if you choose a favourites folder inside Google Drive for desktop (or a similar sync client), that client uploads the files.

Questions: open an issue at https://github.com/andrei399/photo-sorter/issues.
