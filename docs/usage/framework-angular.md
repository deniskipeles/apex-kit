# Using ApexKit with Angular

This guide covers how to integrate the ApexKit SDK into an Angular application using Services and Observables.

## Installation

```bash
npm install @apexkit/sdk
```

## Creating an ApexKit Service

The best way to use ApexKit in Angular is to wrap it in a service.

```typescript
// src/app/services/apexkit.service.ts
import { Injectable } from '@angular/core';
import { ApexKit } from '@apexkit/sdk';
import { environment } from '../../environments/environment';

@Injectable({
  providedIn: 'root'
})
export class ApexKitService {
  public apex: ApexKit;

  constructor() {
    this.apex = new ApexKit(environment.apexkitUrl);
  }
}
```

## Authentication Service

```typescript
// src/app/services/auth.service.ts
import { Injectable } from '@angular/core';
import { BehaviorSubject } from 'rxjs';
import { ApexKitService } from './apexkit.service';
import { User } from '@apexkit/sdk';

@Injectable({
  providedIn: 'root'
})
export class AuthService {
  private userSubject = new BehaviorSubject<User | null>(null);
  public user$ = this.userSubject.asObservable();

  constructor(private apexService: ApexKitService) {
    this.userSubject.next(this.apexService.apex.auth.getUser());
  }

  async login(email: string, pass: string) {
    const res = await this.apexService.apex.auth.login(email, pass);
    this.userSubject.next(res.user);
    return res;
  }

  logout() {
    this.apexService.apex.auth.logout();
    this.userSubject.next(null);
  }
}
```

## Fetching Data in a Component

```typescript
// src/app/components/post-list.component.ts
import { Component, OnInit } from '@angular/core';
import { ApexKitService } from '../services/apexkit.service';
import { BaseRecord } from '@apexkit/sdk';

@Component({
  selector: 'app-post-list',
  template: `
    <div *ngIf="loading">Loading...</div>
    <ul>
      <li *ngFor="let post of posts">{{ post['title'] }}</li>
    </ul>
  `
})
export class PostListComponent implements OnInit {
  posts: BaseRecord[] = [];
  loading = true;

  constructor(private apexService: ApexKitService) {}

  async ngOnInit() {
    try {
      const result = await this.apexService.apex.collection('posts').list();
      this.posts = result.items;
    } finally {
      this.loading = false;
    }
  }
}
```

## Realtime with RxJS

You can wrap ApexKit's realtime listeners into an RxJS Observable.

```typescript
import { Observable } from 'rxjs';
import { ApexKitRealtimeWSClient } from '@apexkit/sdk';

getRealtimeUpdates(collectionId: string): Observable<any> {
  return new Observable(observer => {
    const realtime = new ApexKitRealtimeWSClient(this.apex.baseUrl, this.apex.getToken());
    realtime.connect();
    realtime.subscribe({ collectionId });

    const unsubscribe = realtime.onEvent(msg => {
      observer.next(msg);
    });

    return () => {
      unsubscribe();
      realtime.disconnect();
    };
  });
}
```
